#!/usr/bin/env python3
"""iMessage 连接器:chat.db ⇄ gqy daemon。

只做 I/O:读本用户的 ~/Library/Messages/chat.db 收消息,经通用连接器协议
(gqy-connector/1,本机 WebSocket)交给 daemon;daemon 让发什么,就用 osascript
让「信息」发出去。会话、指令、拆气泡、模型、记忆、语音都在 daemon 里
(src/platforms/connector/,方案稿 docs/design/2026-09-26-connector-protocol.md)。

联系人白名单、主人、气泡设置在 daemon 配置的 platforms.connectors.imessage;
这里的 ~/.gqy/config/imessage.json 只管连哪、口令、轮询。

daemon 回 ack 才推进读取水位:daemon 重启或断线时,没确认的消息重连后重发,
不会丢。只依赖系统自带的 Python 3.9 标准库。运行方式见同目录 README.md。
"""
from __future__ import annotations

import base64
import json
import logging
import os
import queue
import re
import shutil
import socket
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time
import uuid
from urllib.parse import urlsplit

HOME = os.path.expanduser("~")
CONFIG_PATH = os.environ.get(
    "GQY_IMESSAGE_CONFIG", os.path.join(HOME, ".gqy", "config", "imessage.json")
)
STATE_PATH = os.path.join(HOME, ".gqy", "state", "imessage-bridge.json")
CHAT_DB = os.path.join(HOME, "Library", "Messages", "chat.db")
SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))

PROTOCOL = "gqy-connector/1"
CONNECTOR_VERSION = "2"
CAPABILITIES = {
    "reaction_in": True,
    "reaction_out": False,
    "image_out": True,
    "audio_out": True,
    "file_out": True,
    "group": False,
}

# chat.date 是 2001-01-01 起的纳秒数
APPLE_EPOCH = 978307200
# chat.style:45 = 一对一,43 = 群聊
CHAT_STYLE_GROUP = 43
# message.associated_message_type:2000–2006 是加点按回应,3000 起是撤回点按回应
TAPBACKS = {2000: "❤️", 2001: "👍", 2002: "👎", 2003: "😂", 2004: "‼️", 2005: "❓"}
TAPBACK_CUSTOM = 2006
# 与 daemon 的单个附件上限一致(src/platforms/connector/protocol.rs)
MAX_ATTACHMENT_BYTES = 16 * 1024 * 1024

DEFAULT_CONFIG = {
    "enabled": False,
    "url": "ws://127.0.0.1:8300/api/connector/ws?platform=imessage",
    "token": "",
    "poll_seconds": 2,
    "max_backlog_minutes": 30,
}

log = logging.getLogger("imessage-connector")


# ---------------------------------------------------------------------------
# 配置
# ---------------------------------------------------------------------------


def normalize_handle(raw: str) -> str:
    """手机号去掉空格横线,11 位国内号补 +86;邮箱转小写(与 daemon 同一口径)。"""
    value = raw.strip()
    if "@" in value:
        return value.lower()
    digits = re.sub(r"[\s\-()]", "", value)
    if re.fullmatch(r"1\d{10}", digits):
        digits = "+86" + digits
    return digits


def mask_handle(handle: str) -> str:
    if "@" in handle:
        name, _, domain = handle.partition("@")
        return name[:2] + "***@" + domain
    return handle[:4] + "****" + handle[-4:] if len(handle) > 8 else "****"


def load_config() -> dict:
    data = dict(DEFAULT_CONFIG)
    try:
        with open(CONFIG_PATH, encoding="utf-8") as f:
            data.update(json.load(f))
    except FileNotFoundError:
        log.warning("config not found: %s (connector stays disabled)", CONFIG_PATH)
    data["enabled"] = bool(data["enabled"])
    data["url"] = str(data["url"]).strip()
    data["token"] = str(data["token"]).strip()
    data["poll_seconds"] = max(0.5, float(data["poll_seconds"]))
    data["max_backlog_minutes"] = int(data["max_backlog_minutes"])
    return data


class ConfigWatcher:
    """每次取用时按 mtime 判断要不要重读,改完配置即生效。"""

    def __init__(self) -> None:
        self._lock = threading.Lock()
        self._config = load_config()
        self._mtime = self._stat()

    @staticmethod
    def _stat():
        try:
            return os.stat(CONFIG_PATH).st_mtime
        except OSError:
            return None

    def get(self) -> dict:
        with self._lock:
            mtime = self._stat()
            if mtime != self._mtime:
                try:
                    self._config = load_config()
                    log.info("config reloaded: enabled=%s", self._config["enabled"])
                except (ValueError, KeyError, TypeError) as error:
                    log.error("config invalid, keeping previous: %s", error)
                self._mtime = mtime
            return self._config


# ---------------------------------------------------------------------------
# 水位:daemon 确认过的 ROWID 才算处理完
# ---------------------------------------------------------------------------


class Watermark:
    """poller 读到哪里(seen)与 daemon 确认到哪里(durable)分开记。

    落盘的是 durable:没收到 ack 的消息不算处理完,重启后会重新读到、重新发。
    """

    def __init__(self, initial: int) -> None:
        self.seen = initial
        self._pending: set = set()
        self._lock = threading.Lock()
        self._durable = initial

    def claim(self, rowid: int) -> None:
        with self._lock:
            self._pending.add(rowid)

    def release(self, rowid: int) -> None:
        with self._lock:
            self._pending.discard(rowid)
        self.flush()

    def flush(self) -> None:
        with self._lock:
            durable = min(self._pending) - 1 if self._pending else self.seen
            if durable == self._durable:
                return
            self._durable = durable
        save_state({"last_rowid": durable})

    def idle(self) -> bool:
        with self._lock:
            return not self._pending


def load_state() -> dict:
    try:
        with open(STATE_PATH, encoding="utf-8") as f:
            return json.load(f)
    except (FileNotFoundError, ValueError):
        return {}


def save_state(state: dict) -> None:
    os.makedirs(os.path.dirname(STATE_PATH), exist_ok=True)
    tmp = STATE_PATH + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(state, f)
    os.replace(tmp, STATE_PATH)


# ---------------------------------------------------------------------------
# chat.db 读取
# ---------------------------------------------------------------------------


def open_chat_db() -> sqlite3.Connection:
    # 不能用 immutable=1:那样读不到 WAL 里新写入的消息
    conn = sqlite3.connect(f"file:{CHAT_DB}?mode=ro", uri=True, timeout=5)
    conn.row_factory = sqlite3.Row
    return conn


def decode_attributed_body(blob) -> str:
    """从 typedstream 编码的 NSAttributedString 里取出正文。

    结构是 ... "NSString" <类头若干字节> '+' <长度> <UTF-8 正文>。
    长度一字节;0x81 表示后跟 2 字节小端长度,0x82 表示后跟 4 字节。
    """
    if not blob:
        return ""
    data = bytes(blob)
    start = data.find(b"NSString")
    if start < 0:
        return ""
    plus = data.find(b"+", start + len(b"NSString"))
    if plus < 0:
        return ""
    i = plus + 1
    if i >= len(data):
        return ""
    marker = data[i]
    i += 1
    if marker == 0x81:
        length = int.from_bytes(data[i : i + 2], "little")
        i += 2
    elif marker == 0x82:
        length = int.from_bytes(data[i : i + 4], "little")
        i += 4
    else:
        length = marker
    return data[i : i + length].decode("utf-8", errors="replace")


def max_rowid(conn: sqlite3.Connection) -> int:
    row = conn.execute("SELECT MAX(ROWID) AS max FROM message").fetchone()
    return int(row["max"] or 0)


NEW_MESSAGES_SQL = """
SELECT m.ROWID AS rowid, m.text, m.attributedBody, m.is_from_me, m.date,
       m.associated_message_type, m.associated_message_guid, m.thread_originator_guid,
       m.item_type, m.cache_has_attachments,
       h.id AS handle, c.style AS chat_style
FROM message m
LEFT JOIN handle h ON h.ROWID = m.handle_id
LEFT JOIN chat_message_join cmj ON cmj.message_id = m.ROWID
LEFT JOIN chat c ON c.ROWID = cmj.chat_id
WHERE m.ROWID > ?
ORDER BY m.ROWID
LIMIT 200
"""

ATTACHMENTS_SQL = """
SELECT a.filename, a.mime_type, a.transfer_name
FROM attachment a
JOIN message_attachment_join maj ON maj.attachment_id = a.ROWID
WHERE maj.message_id = ?
"""


def message_quote(conn: sqlite3.Connection, guid: str):
    """按 guid 取一条消息,返回协议里的 Quote(id/text/from_me)。取不到返回 None。"""
    if not guid:
        return None
    row = conn.execute(
        "SELECT text, attributedBody, is_from_me, cache_has_attachments FROM message WHERE guid = ?",
        (guid,),
    ).fetchone()
    if row is None:
        return None
    text = (row["text"] or decode_attributed_body(row["attributedBody"])).replace("￼", "")
    text = " ".join(text.split())
    if not text and row["cache_has_attachments"]:
        text = "[image]"
    return {"id": guid, "text": text, "from_me": bool(row["is_from_me"])}


def tapback(conn: sqlite3.Connection, row):
    """点按回应 → (表情, 被点的那条)。目标 guid 形如 p:0/GUID 或 bp:GUID。"""
    kind = int(row["associated_message_type"])
    emoji = TAPBACKS.get(kind, "")
    if kind == TAPBACK_CUSTOM:
        try:  # 自定义表情回应(macOS 15+ 才有这一列)
            found = conn.execute(
                "SELECT associated_message_emoji FROM message WHERE ROWID = ?", (row["rowid"],)
            ).fetchone()
            emoji = (found[0] if found else "") or ""
        except sqlite3.Error:
            emoji = ""
    if not emoji:
        return None
    target = (row["associated_message_guid"] or "").split("/")[-1]
    if target.startswith("bp:"):
        target = target[3:]
    return emoji, message_quote(conn, target)


def wait_for_file(path: str, seconds: float = 20) -> bool:
    """附件行先落库、文件后下载完,等一会儿。"""
    deadline = time.time() + seconds
    while time.time() < deadline:
        if os.path.exists(path) and os.path.getsize(path) > 0:
            return True
        time.sleep(1)
    return os.path.exists(path)


IMAGE_EXTENSIONS = (".jpg", ".jpeg", ".png", ".gif", ".webp")


def read_image(path: str, mime: str):
    """图片 → (mime, bytes)。HEIC 等先用 sips 转成 JPEG。不是图片返回 None。"""
    lower = path.lower()
    if not (mime.startswith("image/") or lower.endswith(IMAGE_EXTENSIONS + (".heic", ".heif"))):
        return None
    if lower.endswith(IMAGE_EXTENSIONS):
        with open(path, "rb") as f:
            data = f.read(MAX_ATTACHMENT_BYTES + 1)
        return (mime or "image/jpeg"), data
    with tempfile.TemporaryDirectory() as workdir:
        out = os.path.join(workdir, "converted.jpg")
        result = subprocess.run(
            ["/usr/bin/sips", "-s", "format", "jpeg", path, "--out", out],
            capture_output=True,
            text=True,
            timeout=60,
        )
        if result.returncode != 0:
            raise RuntimeError("sips conversion failed")
        with open(out, "rb") as f:
            return "image/jpeg", f.read(MAX_ATTACHMENT_BYTES + 1)


def attachment_entry(path: str, mime: str, name: str) -> dict:
    """一个附件 → 协议里的 attachment。只有图片带内容,语音和文件 daemon 只要名字。"""
    lower = path.lower()
    if mime.startswith("audio/") or lower.endswith((".m4a", ".caf", ".mp3", ".wav", ".aac", ".ogg")):
        return {"kind": "audio", "name": name, "mime": mime}
    entry = {"kind": "image", "name": name, "mime": mime}
    try:
        if not wait_for_file(path):
            raise RuntimeError("attachment not downloaded")
        image = read_image(path, mime)
        if image is None:
            return {"kind": "file", "name": name, "mime": mime}
        entry["mime"], data = image
        if len(data) > MAX_ATTACHMENT_BYTES:
            raise RuntimeError("image too large")
        entry["data"] = base64.b64encode(data).decode("ascii")
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        entry["error"] = str(error)
    return entry


def fetch_new(conn: sqlite3.Connection, after: int):
    """返回 (新水位, [(rowid, 事件帧)])。群聊、自己发的、非正文行只推进水位。"""
    rows = conn.execute(NEW_MESSAGES_SQL, (after,)).fetchall()
    top = after
    events = []
    seen = set()
    for row in rows:
        rowid = int(row["rowid"])
        top = max(top, rowid)
        # 一条消息可能因 chat 关联出现多行
        if rowid in seen:
            continue
        seen.add(rowid)
        if row["is_from_me"] or not row["handle"] or row["chat_style"] == CHAT_STYLE_GROUP:
            continue
        handle = normalize_handle(row["handle"])
        base = {
            "type": "event",
            "id": str(rowid),
            "conversation": {"kind": "private", "id": handle},
            "sender": {"id": handle},
        }
        kind = int(row["associated_message_type"] or 0)
        if kind:
            # 加点按回应报上去;撤回回应、贴纸等其余关联消息跳过
            found = tapback(conn, row) if 2000 <= kind <= TAPBACK_CUSTOM else None
            if found:
                emoji, target = found
                events.append((rowid, dict(base, kind="reaction", reaction=emoji, target=target)))
            continue
        # 群事件等不是正文消息
        if row["item_type"]:
            continue
        text = row["text"] or decode_attributed_body(row["attributedBody"])
        # U+FFFC 是附件在正文里的占位符
        text = text.replace("￼", "").strip()
        attachments = []
        if row["cache_has_attachments"]:
            for a in conn.execute(ATTACHMENTS_SQL, (rowid,)).fetchall():
                if a["filename"]:
                    path = os.path.expanduser(a["filename"])
                    name = a["transfer_name"] or os.path.basename(path)
                    attachments.append(attachment_entry(path, a["mime_type"] or "", name))
        if not text and not attachments:
            continue
        date = row["date"] or 0
        seconds = date / 1e9 if date > 1e12 else date
        event = dict(
            base,
            kind="message",
            text=text,
            attachments=attachments,
            timestamp=int(seconds + APPLE_EPOCH),
        )
        quote = message_quote(conn, row["thread_originator_guid"])
        if quote:
            event["reply_to"] = quote
        events.append((rowid, event))
    return top, events


# ---------------------------------------------------------------------------
# 发送
# ---------------------------------------------------------------------------

SEND_TEXT_SCRIPT = """
on run argv
    set msgText to item 1 of argv
    set handleId to item 2 of argv
    tell application "Messages"
        set acct to 1st account whose service type = iMessage
        send msgText to participant handleId of acct
    end tell
end run
"""

SEND_FILE_SCRIPT = """
on run argv
    set filePath to item 1 of argv
    set handleId to item 2 of argv
    tell application "Messages"
        set acct to 1st account whose service type = iMessage
        send (POSIX file filePath) to participant handleId of acct
    end tell
end run
"""

# macOS 15+ 的 Messages 受沙盒限制,只能读少数目录里的文件。放错位置时 osascript
# 照样返回成功,附件却发不出去。本机(macOS 27)实测:
# - ~/Pictures/...:Messages 读不到,消息里没有附件
# - ~/Library/Messages/Attachments/...:能读到,但那是 Messages 自己的附件库,
#   手机端收不到图
# 所以放在 ~/Library/Messages 下、附件库之外的暂存目录(anthropics/claude-plugins-official#1113)。
OUTBOX_DIR = os.path.join(HOME, "Library", "Messages", ".gqy-send-staging")


def osascript_error(result) -> str:
    """只保留 AppleScript 错误码,不把 stderr 原文(可能含消息正文)写进日志。"""
    codes = re.findall(r"\((-?\d+)\)", result.stderr or "")
    return f"osascript exit {result.returncode}, code {codes[-1] if codes else 'unknown'}"


def run_osascript(script: str, first: str, handle: str, timeout: int) -> None:
    # 正文和路径走 argv,不拼进脚本源码,无需转义
    result = subprocess.run(
        ["/usr/bin/osascript", "-", first, handle],
        input=script,
        capture_output=True,
        text=True,
        timeout=timeout,
    )
    if result.returncode != 0:
        raise RuntimeError(osascript_error(result))


def cleanup_outbox(max_age_seconds: float = 3600) -> None:
    """删掉一小时前的暂存副本。Messages 发送时已经把文件另存进自己的附件库。"""
    try:
        entries = os.listdir(OUTBOX_DIR)
    except OSError:
        return
    cutoff = time.time() - max_age_seconds
    for name in entries:
        entry = os.path.join(OUTBOX_DIR, name)
        try:
            if os.path.isdir(entry) and os.stat(entry).st_mtime < cutoff:
                shutil.rmtree(entry)
        except OSError:
            continue


def safe_file_name(name: str, fallback: str) -> str:
    name = os.path.basename(name or "").strip().lstrip(".")
    name = re.sub(r"[^\w.\-]+", "_", name)
    return name[:80] or fallback


def stage_attachment(part: dict) -> str:
    """把 daemon 发来的附件写进 Messages 读得到的暂存目录(见 OUTBOX_DIR)。

    语音转成 Apple 原生的 caf(opus),手机上能直接播;转不了就发原文件。
    """
    cleanup_outbox()
    data = base64.b64decode(part.get("data", ""), validate=True)
    if len(data) > MAX_ATTACHMENT_BYTES:
        raise RuntimeError("attachment too large")
    target_dir = os.path.join(OUTBOX_DIR, uuid.uuid4().hex)
    os.makedirs(target_dir, exist_ok=True)
    kind = part.get("kind")
    target = os.path.join(target_dir, safe_file_name(part.get("name", ""), kind or "attachment"))
    with open(target, "wb") as f:
        f.write(data)
    if kind == "audio" and not target.lower().endswith((".caf", ".m4a")):
        converted = os.path.splitext(target)[0] + ".caf"
        result = subprocess.run(
            ["/usr/bin/afconvert", "-f", "caff", "-d", "opus", target, converted],
            capture_output=True,
            timeout=30,
        )
        if result.returncode == 0 and os.path.exists(converted) and os.path.getsize(converted) > 0:
            os.unlink(target)
            return converted
    return target


def send_part(to: str, part: dict) -> None:
    if part.get("kind") == "text":
        run_osascript(SEND_TEXT_SCRIPT, part.get("text", ""), to, 30)
    else:
        run_osascript(SEND_FILE_SCRIPT, stage_attachment(part), to, 60)


DELIVERY_SQL = """
SELECT m.ROWID AS rowid, m.error, m.is_sent, m.is_delivered, m.cache_has_attachments,
       a.transfer_state
FROM message m
LEFT JOIN message_attachment_join maj ON maj.message_id = m.ROWID
LEFT JOIN attachment a ON a.ROWID = maj.attachment_id
WHERE m.ROWID > ? AND m.is_from_me = 1
ORDER BY m.ROWID
"""


def check_delivery(handle: str, before_rowid: int, wait_seconds: float = 30) -> None:
    """osascript 成功不代表送达:回查 chat.db 里新写入的己方消息,只记日志。"""
    deadline = time.time() + wait_seconds
    rows = []
    while time.time() < deadline:
        time.sleep(2)
        try:
            conn = open_chat_db()
            try:
                rows = conn.execute(DELIVERY_SQL, (before_rowid,)).fetchall()
            finally:
                conn.close()
        except sqlite3.Error as error:
            log.warning("delivery check failed: %s", error)
            return
        if rows and all(r["is_delivered"] or r["error"] for r in rows):
            break
    if not rows:
        log.warning("sent message to %s did not appear in chat.db", mask_handle(handle))
    for r in rows:
        level = logging.ERROR if r["error"] else logging.INFO
        log.log(
            level,
            "sent rowid=%s error=%s is_sent=%s delivered=%s attachment=%s transfer_state=%s",
            r["rowid"],
            r["error"],
            r["is_sent"],
            r["is_delivered"],
            r["cache_has_attachments"],
            r["transfer_state"],
        )


# ---------------------------------------------------------------------------
# WebSocket 客户端(RFC 6455 的最小子集:文本帧、分片、ping/pong、close)
# ---------------------------------------------------------------------------


def apply_mask(payload: bytes, mask: bytes) -> bytes:
    """按 RFC 6455 异或掩码。整块做大整数异或,几十 MB 的图也不会逐字节慢慢算。"""
    if not payload:
        return payload
    size = len(payload)
    key = (mask * (size // 4 + 1))[:size]
    return (int.from_bytes(payload, "big") ^ int.from_bytes(key, "big")).to_bytes(size, "big")


class ConnectionClosed(Exception):
    pass


class HandshakeRejected(Exception):
    pass


class WebSocket:
    def __init__(self, url: str, token: str, timeout: float = 10) -> None:
        parts = urlsplit(url)
        if parts.scheme != "ws":
            raise ValueError("only ws:// URLs are supported")
        host = parts.hostname or "127.0.0.1"
        port = parts.port or 80
        path = (parts.path or "/") + (f"?{parts.query}" if parts.query else "")
        self.sock = socket.create_connection((host, port), timeout=timeout)
        self._send_lock = threading.Lock()
        key = base64.b64encode(os.urandom(16)).decode("ascii")
        request = (
            f"GET {path} HTTP/1.1\r\n"
            f"Host: {host}:{port}\r\n"
            "Upgrade: websocket\r\n"
            "Connection: Upgrade\r\n"
            f"Sec-WebSocket-Key: {key}\r\n"
            "Sec-WebSocket-Version: 13\r\n"
            f"Authorization: Bearer {token}\r\n\r\n"
        )
        self.sock.sendall(request.encode("utf-8"))
        response = b""
        while b"\r\n\r\n" not in response:
            chunk = self.sock.recv(4096)
            if not chunk:
                raise ConnectionClosed("closed during handshake")
            response += chunk
            if len(response) > 65536:
                raise HandshakeRejected("oversized handshake response")
        head, _, self._buffer = response.partition(b"\r\n\r\n")
        status = head.split(b"\r\n", 1)[0].decode("latin-1")
        if " 101 " not in status + " ":
            raise HandshakeRejected(status)
        # daemon 30 秒发一次 ping:超过 95 秒什么都没收到,当它没了,重连
        self.sock.settimeout(95)

    def _read_exact(self, count: int) -> bytes:
        while len(self._buffer) < count:
            chunk = self.sock.recv(max(65536, count - len(self._buffer)))
            if not chunk:
                raise ConnectionClosed("connection closed")
            self._buffer += chunk
        data, self._buffer = self._buffer[:count], self._buffer[count:]
        return data

    def _send_frame(self, opcode: int, payload: bytes) -> None:
        header = bytearray([0x80 | opcode])
        length = len(payload)
        if length < 126:
            header.append(0x80 | length)
        elif length < 65536:
            header.append(0x80 | 126)
            header += length.to_bytes(2, "big")
        else:
            header.append(0x80 | 127)
            header += length.to_bytes(8, "big")
        mask = os.urandom(4)
        with self._send_lock:
            self.sock.sendall(bytes(header) + mask + apply_mask(payload, mask))

    def send_text(self, text: str) -> None:
        self._send_frame(0x1, text.encode("utf-8"))

    def recv_text(self) -> str:
        """下一条完整的文本消息。ping 自动回 pong,close 抛 ConnectionClosed。"""
        message = b""
        while True:
            first, second = self._read_exact(2)
            fin, opcode = first & 0x80, first & 0x0F
            length = second & 0x7F
            if length == 126:
                length = int.from_bytes(self._read_exact(2), "big")
            elif length == 127:
                length = int.from_bytes(self._read_exact(8), "big")
            mask = self._read_exact(4) if second & 0x80 else None
            payload = self._read_exact(length)
            if mask:
                payload = apply_mask(payload, mask)
            if opcode == 0x8:
                raise ConnectionClosed("server closed the connection")
            if opcode == 0x9:
                self._send_frame(0xA, payload)
                continue
            if opcode == 0xA:
                continue
            message += payload
            if fin:
                return message.decode("utf-8")

    def close(self) -> None:
        try:
            self._send_frame(0x8, b"")
        except OSError:
            pass
        try:
            self.sock.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        self.sock.close()


# ---------------------------------------------------------------------------
# 连接器:收发两头接到 daemon
# ---------------------------------------------------------------------------


class Connector:
    """连 daemon、握手、发事件、收 ack 与发送请求。断线指数退避重连,
    重连后把没收到 ack 的事件按顺序重发。"""

    def __init__(self, watcher: ConfigWatcher, watermark: Watermark) -> None:
        self.watcher = watcher
        self.watermark = watermark
        self._lock = threading.Lock()
        self._pending: dict = {}  # rowid → 事件帧(JSON),等 ack
        self._ws = None
        self._ready = threading.Event()
        self._sends: queue.Queue = queue.Queue()
        threading.Thread(target=self._connection_loop, name="connection", daemon=True).start()
        threading.Thread(target=self._send_loop, name="sender", daemon=True).start()

    def submit(self, rowid: int, event: dict) -> None:
        frame = json.dumps(event, ensure_ascii=False)
        self.watermark.claim(rowid)
        with self._lock:
            self._pending[rowid] = frame
            ws = self._ws if self._ready.is_set() else None
        if ws is not None:
            self._send(ws, frame)

    def idle(self) -> bool:
        with self._lock:
            return not self._pending and self._sends.empty()

    def _send(self, ws: WebSocket, frame: str) -> None:
        try:
            ws.send_text(frame)
        except OSError as error:
            log.warning("sending to the daemon failed: %s", error)
            ws.close()

    def _connection_loop(self) -> None:
        backoff = 1.0
        while True:
            config = self.watcher.get()
            if not config["enabled"]:
                time.sleep(5)
                continue
            if not config["token"]:
                log.error("no token in %s; set it to platforms.connectors.imessage.token", CONFIG_PATH)
                time.sleep(30)
                continue
            try:
                ws = WebSocket(config["url"], config["token"])
            except HandshakeRejected as error:
                log.error(
                    "daemon refused the connection (%s); check the token and that "
                    "platforms.connectors.imessage is enabled",
                    error,
                )
                time.sleep(30)
                continue
            except (OSError, ConnectionClosed, ValueError) as error:
                log.info("daemon not reachable (%s); retrying in %.0fs", error, backoff)
                time.sleep(backoff)
                backoff = min(backoff * 2, 30)
                continue
            backoff = 1.0
            self._serve(ws, config)
            self._ready.clear()
            with self._lock:
                self._ws = None
            time.sleep(1)

    def _serve(self, ws: WebSocket, config: dict) -> None:
        hello = {
            "type": "hello",
            "protocol": PROTOCOL,
            "platform": "imessage",
            "display_name": "iMessage",
            "connector": {"name": "gqy-imessage", "version": CONNECTOR_VERSION},
            "capabilities": CAPABILITIES,
        }
        try:
            ws.send_text(json.dumps(hello))
            while True:
                if self.watcher.get() is not config:
                    log.info("config changed; reconnecting")
                    break
                frame = json.loads(ws.recv_text())
                kind = frame.get("type")
                if kind == "welcome":
                    log.info("connected to the daemon (connection %s)", frame.get("connection"))
                    with self._lock:
                        self._ws = ws
                        self._ready.set()
                        backlog = [self._pending[rowid] for rowid in sorted(self._pending)]
                    for pending in backlog:
                        self._send(ws, pending)
                elif kind == "ack":
                    rowid = int(frame.get("id", "0"))
                    with self._lock:
                        self._pending.pop(rowid, None)
                    self.watermark.release(rowid)
                elif kind == "send":
                    self._sends.put((ws, frame))
                elif kind == "ping":
                    ws.send_text('{"type":"pong"}')
                elif kind == "error":
                    log.error("daemon error %s: %s", frame.get("code"), frame.get("message"))
        except (OSError, ConnectionClosed, ValueError) as error:
            log.info("disconnected from the daemon: %s", error)
        finally:
            ws.close()

    def _send_loop(self) -> None:
        while True:
            ws, frame = self._sends.get()
            part = frame.get("part") or {}
            to = str(frame.get("to", ""))
            result = {"type": "send_result", "req": frame.get("req"), "ok": True}
            try:
                conn = open_chat_db()
                try:
                    before = max_rowid(conn)
                finally:
                    conn.close()
                send_part(to, part)
                threading.Thread(
                    target=check_delivery, args=(to, before), name="delivery", daemon=True
                ).start()
            except (OSError, RuntimeError, ValueError, sqlite3.Error, subprocess.SubprocessError) as error:
                log.warning("send %s to %s failed: %s", part.get("kind"), mask_handle(to), error)
                result = {"type": "send_result", "req": frame.get("req"), "ok": False, "error": str(error)}
            try:
                ws.send_text(json.dumps(result))
            except OSError as error:
                log.warning("could not report a send result: %s", error)


# ---------------------------------------------------------------------------
# 主循环
# ---------------------------------------------------------------------------


class SelfReloader:
    """本脚本被改动且能编译通过时,空闲下来就用新代码替换本进程。"""

    def __init__(self) -> None:
        self.path = os.path.abspath(__file__)
        self.mtime = self._stat()

    def _stat(self):
        try:
            return os.stat(self.path).st_mtime
        except OSError:
            return None

    def changed(self) -> bool:
        mtime = self._stat()
        if mtime == self.mtime:
            return False
        try:
            with open(self.path, encoding="utf-8") as f:
                compile(f.read(), self.path, "exec")
        except (SyntaxError, ValueError, OSError) as error:
            log.error("script changed but does not compile, keeping old code: %s", error)
            self.mtime = mtime
            return False
        return True

    def exec(self) -> None:
        log.info("script changed, reloading")
        os.execv(sys.executable, [sys.executable, self.path] + sys.argv[1:])


def db_signature():
    """chat.db 与 -wal 的 (mtime, size),没变就不查库。"""
    sig = []
    for suffix in ("", "-wal"):
        try:
            st = os.stat(CHAT_DB + suffix)
            sig.append((st.st_mtime_ns, st.st_size))
        except OSError:
            sig.append(None)
    return tuple(sig)


def initial_rowid(conn: sqlite3.Connection, config: dict) -> int:
    current = max_rowid(conn)
    saved = load_state().get("last_rowid")
    if saved is None or saved > current:
        return current
    # 停机太久时不回灌陈旧消息
    cutoff_ns = (time.time() - APPLE_EPOCH - config["max_backlog_minutes"] * 60) * 1e9
    row = conn.execute(
        "SELECT MAX(ROWID) AS max FROM message WHERE ROWID > ? AND date < ?",
        (saved, cutoff_ns),
    ).fetchone()
    return max(int(saved), int(row["max"] or 0))


LOG_PATH = os.path.join(HOME, ".gqy", "cache", "logs", "imessage-bridge.log")
LOG_MAX_BYTES = 2 * 1024 * 1024
LOG_BACKUPS = 2


def setup_logging() -> None:
    """日志自己轮转:单个 2 MB,保留 2 份旧的,总量封顶约 6 MB。

    launchd 的 stdout/stderr 只接意外崩溃的回溯(见 install.sh),平时是空的。
    """
    from logging.handlers import RotatingFileHandler

    os.makedirs(os.path.dirname(LOG_PATH), exist_ok=True)
    handler = RotatingFileHandler(
        LOG_PATH, maxBytes=LOG_MAX_BYTES, backupCount=LOG_BACKUPS, encoding="utf-8"
    )
    handler.setFormatter(logging.Formatter("%(asctime)s %(levelname)s %(threadName)s %(message)s"))
    root = logging.getLogger()
    root.handlers[:] = [handler]
    root.setLevel(logging.INFO)


def running_app_path() -> str:
    """权限要授给谁。经专用启动器运行时就是启动器本身。

    Xcode 自带的 python3 会转进 Python.app 运行,sys.executable 报的却是
    bin/python3.9,按它授权无效。取进程实际映像,落在 .app 里就给 .app 路径。
    """
    launcher = os.environ.get("GQY_IMESSAGE_LAUNCHER")
    if launcher:
        return launcher
    try:
        image = subprocess.run(
            ["/bin/ps", "-o", "comm=", "-p", str(os.getpid())],
            capture_output=True,
            text=True,
            timeout=5,
        ).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        image = ""
    image = image or os.path.realpath(sys.executable)
    marker = ".app/"
    return image[: image.rindex(marker) + len(marker) - 1] if marker in image else image


def main() -> int:
    setup_logging()
    log.info("starting, python=%s", os.path.realpath(sys.executable))
    watcher = ConfigWatcher()
    reloader = SelfReloader()

    try:
        conn = open_chat_db()
        conn.execute("SELECT 1 FROM message LIMIT 1").fetchone()
    except sqlite3.Error as error:
        log.error(
            "cannot read %s: %s. Grant Full Disk Access to %s and restart.",
            CHAT_DB,
            error,
            running_app_path(),
        )
        # 睡一会儿再退出,避免 launchd 高频拉起
        time.sleep(60)
        return 1

    watermark = Watermark(initial_rowid(conn, watcher.get()))
    watermark.flush()
    log.info("watching from rowid %d", watermark.seen)
    connector = Connector(watcher, watermark)
    last_sig = None

    while True:
        config = watcher.get()
        sig = db_signature()
        if sig != last_sig:
            last_sig = sig
            try:
                top, events = fetch_new(conn, watermark.seen)
            except sqlite3.Error as error:
                log.warning("query failed, reopening db: %s", error)
                conn.close()
                time.sleep(config["poll_seconds"])
                conn = open_chat_db()
                last_sig = None
                continue
            # 关着的时候照样推进水位:打开时不回灌关着期间的消息
            if config["enabled"]:
                for rowid, event in events:
                    connector.submit(rowid, event)
            watermark.seen = top
            watermark.flush()

        if watermark.idle() and connector.idle() and reloader.changed():
            conn.close()
            reloader.exec()

        # 按 0.25s 切片检查 chat.db 有没有变,一变就马上去读,兼顾省电与响应
        elapsed = 0.0
        while elapsed < config["poll_seconds"]:
            time.sleep(0.25)
            elapsed += 0.25
            if db_signature() != last_sig:
                break


if __name__ == "__main__":
    sys.exit(main())
