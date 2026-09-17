#!/usr/bin/env python3
"""iMessage ⇄ 顾清影 桥接。

读本用户的 ~/Library/Messages/chat.db 收消息,交给 `gqy ask` 跑回合,
再用 osascript 让 Messages 发回去。只认配置里白名单联系人的私聊。

运行方式见同目录 README.md。只依赖系统自带的 Python 3.9 标准库。
"""
from __future__ import annotations

import json
import logging
import os
import queue
import re
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time
from dataclasses import dataclass, field

HOME = os.path.expanduser("~")
CONFIG_PATH = os.environ.get(
    "GQY_IMESSAGE_CONFIG", os.path.join(HOME, ".gqy", "config", "imessage.json")
)
STATE_PATH = os.path.join(HOME, ".gqy", "state", "imessage-bridge.json")
CHAT_DB = os.path.join(HOME, "Library", "Messages", "chat.db")
SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
HINT_PATH = os.path.join(SCRIPT_DIR, "hint.txt")

# chat.date 是 2001-01-01 起的纳秒数
APPLE_EPOCH = 978307200
# chat.style:45 = 一对一,43 = 群聊
CHAT_STYLE_GROUP = 43

DEFAULT_CONFIG = {
    "enabled": False,
    "contacts": [],
    "gqy_bin": os.path.join(HOME, ".cargo", "bin", "gqy"),
    "tools": [
        "web_search",
        "web_fetch",
        "vision_analyze",
        "recall_memories",
        "remember_fact",
        "kb",
        "search_knowledge_base",
        "search_evicted_context",
        "get_exchange_rate",
        "use_meme",
        "album",
    ],
    "poll_seconds": 2,
    "batch_wait_seconds": 3,
    "timeout_seconds": 300,
    "max_backlog_minutes": 30,
    "split_paragraphs": True,
    "max_bubbles": 6,
    "max_memes": 2,
}

log = logging.getLogger("imessage-bridge")


# ---------------------------------------------------------------------------
# 配置
# ---------------------------------------------------------------------------


def normalize_handle(raw: str) -> str:
    """手机号去掉空格横线,11 位国内号补 +86;邮箱转小写。"""
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


@dataclass
class Config:
    enabled: bool
    handle_to_contact: dict  # 规范化 handle → 联系人名
    gqy_bin: str
    tools: list
    poll_seconds: float
    batch_wait_seconds: float
    timeout_seconds: int
    max_backlog_minutes: int
    split_paragraphs: bool
    max_bubbles: int
    max_memes: int


def load_config() -> Config:
    data = dict(DEFAULT_CONFIG)
    try:
        with open(CONFIG_PATH, encoding="utf-8") as f:
            data.update(json.load(f))
    except FileNotFoundError:
        log.warning("config not found: %s (bridge stays disabled)", CONFIG_PATH)
    handle_to_contact = {}
    for contact in data.get("contacts", []):
        name = str(contact.get("name", "")).strip()
        if not name:
            continue
        for handle in contact.get("handles", []):
            handle_to_contact[normalize_handle(handle)] = name
    return Config(
        enabled=bool(data["enabled"]),
        handle_to_contact=handle_to_contact,
        gqy_bin=os.path.expanduser(data["gqy_bin"]),
        tools=list(data["tools"]),
        poll_seconds=max(0.5, float(data["poll_seconds"])),
        batch_wait_seconds=max(0.0, float(data["batch_wait_seconds"])),
        timeout_seconds=int(data["timeout_seconds"]),
        max_backlog_minutes=int(data["max_backlog_minutes"]),
        split_paragraphs=bool(data["split_paragraphs"]),
        max_bubbles=max(1, int(data["max_bubbles"])),
        max_memes=max(0, int(data["max_memes"])),
    )


class ConfigWatcher:
    """每次取用时按 mtime 判断要不要重读,改完配置即生效。"""

    def __init__(self) -> None:
        self._mtime = None
        self._config = load_config()
        self._mtime = self._stat()
        self._lock = threading.Lock()

    @staticmethod
    def _stat():
        try:
            return os.stat(CONFIG_PATH).st_mtime
        except OSError:
            return None

    def get(self) -> Config:
        with self._lock:
            mtime = self._stat()
            if mtime != self._mtime:
                try:
                    self._config = load_config()
                    log.info(
                        "config reloaded: enabled=%s contacts=%d",
                        self._config.enabled,
                        len(set(self._config.handle_to_contact.values())),
                    )
                except (ValueError, KeyError, TypeError) as error:
                    log.error("config invalid, keeping previous: %s", error)
                self._mtime = mtime
            return self._config


# ---------------------------------------------------------------------------
# 状态:已处理完的 ROWID 水位
# ---------------------------------------------------------------------------


class Watermark:
    """poller 读到哪里(seen)与真正处理完到哪里(durable)分开记。

    落盘的是 durable:回合还没跑完的消息不算处理完,进程中途退出后重启会
    重新处理它们,不会悄悄丢掉。
    """

    def __init__(self, initial: int) -> None:
        self.seen = initial
        self._pending: set = set()
        self._lock = threading.Lock()
        self._durable = initial

    def claim(self, rowid: int) -> None:
        with self._lock:
            self._pending.add(rowid)

    def release(self, rowids) -> None:
        with self._lock:
            self._pending.difference_update(rowids)
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


@dataclass
class Inbound:
    rowid: int
    handle: str
    text: str
    date_unix: float
    attachments: list = field(default_factory=list)  # [(path, mime, name)]


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
       m.associated_message_type, m.item_type, m.cache_has_attachments,
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


def fetch_new(conn: sqlite3.Connection, after: int):
    """返回 (新水位, 需要处理的入站消息列表)。其余行只推进水位。"""
    rows = conn.execute(NEW_MESSAGES_SQL, (after,)).fetchall()
    top = after
    inbound = []
    seen = set()
    for row in rows:
        rowid = int(row["rowid"])
        top = max(top, rowid)
        # 一条消息可能因 chat 关联出现多行
        if rowid in seen:
            continue
        seen.add(rowid)
        if row["is_from_me"] or not row["handle"]:
            continue
        # 点按回应、群事件等不是正文消息
        if row["associated_message_type"] or row["item_type"]:
            continue
        if row["chat_style"] == CHAT_STYLE_GROUP:
            continue
        text = row["text"] or decode_attributed_body(row["attributedBody"])
        # U+FFFC 是附件在正文里的占位符
        text = text.replace("￼", "").strip()
        attachments = []
        if row["cache_has_attachments"]:
            for a in conn.execute(ATTACHMENTS_SQL, (rowid,)).fetchall():
                if a["filename"]:
                    attachments.append(
                        (
                            os.path.expanduser(a["filename"]),
                            a["mime_type"] or "",
                            a["transfer_name"] or os.path.basename(a["filename"]),
                        )
                    )
        if not text and not attachments:
            continue
        date = row["date"] or 0
        seconds = date / 1e9 if date > 1e12 else date
        inbound.append(
            Inbound(
                rowid=rowid,
                handle=normalize_handle(row["handle"]),
                text=text,
                date_unix=seconds + APPLE_EPOCH,
                attachments=attachments,
            )
        )
    return top, inbound


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


def osascript_error(result) -> str:
    """只保留 AppleScript 错误码,不把 stderr 原文(可能含消息正文)写进日志。"""
    codes = re.findall(r"\((-?\d+)\)", result.stderr or "")
    return f"osascript exit {result.returncode}, code {codes[-1] if codes else 'unknown'}"


def send_text(handle: str, text: str) -> None:
    # 正文走 argv,不拼进脚本源码,无需转义
    result = subprocess.run(
        ["/usr/bin/osascript", "-", text, handle],
        input=SEND_TEXT_SCRIPT,
        capture_output=True,
        text=True,
        timeout=30,
    )
    if result.returncode != 0:
        raise RuntimeError(osascript_error(result))


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


def stage_for_messages(path: str) -> str:
    """复制到 Messages 读得到的暂存目录再发(见 OUTBOX_DIR)。"""
    import shutil
    import uuid

    target_dir = os.path.join(OUTBOX_DIR, uuid.uuid4().hex)
    os.makedirs(target_dir, exist_ok=True)
    target = os.path.join(target_dir, os.path.basename(path))
    shutil.copyfile(path, target)
    return target


def cleanup_outbox(max_age_seconds: float = 3600) -> None:
    """删掉一小时前的暂存副本。Messages 发送时已经把文件另存进自己的附件库。"""
    import shutil

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


def send_file(handle: str, path: str) -> None:
    cleanup_outbox()
    staged = stage_for_messages(path)
    result = subprocess.run(
        ["/usr/bin/osascript", "-", staged, handle],
        input=SEND_FILE_SCRIPT,
        capture_output=True,
        text=True,
        timeout=60,
    )
    if result.returncode != 0:
        raise RuntimeError(osascript_error(result))


DELIVERY_SQL = """
SELECT m.ROWID AS rowid, m.error, m.is_sent, m.is_delivered, m.cache_has_attachments,
       h.id AS handle, a.transfer_state, a.total_bytes, a.mime_type
FROM message m
LEFT JOIN handle h ON h.ROWID = m.handle_id
LEFT JOIN message_attachment_join maj ON maj.message_id = m.ROWID
LEFT JOIN attachment a ON a.ROWID = maj.attachment_id
WHERE m.ROWID > ? AND m.is_from_me = 1
ORDER BY m.ROWID
"""


def check_delivery(handle: str, before_rowid: int, expected: int, wait_seconds: float = 30) -> None:
    """osascript 成功不代表送达。回查 chat.db 里新写入的己方消息。

    等到条数够了(或超时)再逐条记录状态,附件带上传输状态,便于排查。
    """
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
        # 送达回执通常 1~4 秒内回来,附件消息会慢一点;等到都有结果再记日志
        settled = all(r["is_delivered"] or r["error"] for r in rows)
        if len(rows) >= expected and settled:
            break
    if len(rows) < expected:
        log.warning(
            "only %d of %d sent messages to %s appeared in chat.db",
            len(rows),
            expected,
            mask_handle(handle),
        )
    for r in rows:
        level = logging.ERROR if r["error"] else logging.INFO
        log.log(
            level,
            "sent rowid=%s error=%s is_sent=%s delivered=%s attachment=%s "
            "transfer_state=%s bytes=%s mime=%s handle=%s",
            r["rowid"],
            r["error"],
            r["is_sent"],
            r["is_delivered"],
            r["cache_has_attachments"],
            r["transfer_state"],
            r["total_bytes"],
            r["mime_type"],
            mask_handle(normalize_handle(r["handle"])) if r["handle"] else None,
        )


# ---------------------------------------------------------------------------
# 文本整形
# ---------------------------------------------------------------------------

LINK_RE = re.compile(r"\[([^\]]+)\]\(([^)\s]+)\)")


def markdown_to_plain(text: str) -> str:
    """与 src/platforms/reply.rs 的 markdown_to_plain 同一口径,另外去掉 ~~。"""
    out = []
    in_fence = False
    for line in text.splitlines():
        stripped = line.lstrip()
        if stripped.startswith("```") or stripped.startswith("~~~"):
            in_fence = not in_fence
            continue
        if in_fence:
            out.append(line)
            continue
        if stripped.startswith("#"):
            line = stripped.lstrip("#").lstrip()
        elif stripped.startswith("> "):
            line = stripped[2:]
        line = LINK_RE.sub(r"\1 (\2)", line)
        for token in ("**", "__", "~~", "`"):
            line = line.replace(token, "")
        out.append(line)
    return "\n".join(out).strip()


def split_bubbles(text: str, max_bubbles: int) -> list:
    parts = [p.strip() for p in re.split(r"\n\s*\n", text) if p.strip()]
    if len(parts) <= max_bubbles:
        return parts
    return parts[: max_bubbles - 1] + ["\n\n".join(parts[max_bubbles - 1 :])]


# ---------------------------------------------------------------------------
# 回合
# ---------------------------------------------------------------------------


def prepare_image(path: str, mime: str, workdir: str):
    """HEIC 等转成 JPEG,其余图片复制到 workdir 避免子进程遇到 FDA 权限拦截。"""
    lower = path.lower()
    if not (mime.startswith("image/") or lower.endswith((".heic", ".heif", ".jpg", ".jpeg", ".png", ".gif", ".webp"))):
        return None
    if lower.endswith((".jpg", ".jpeg", ".png", ".gif", ".webp")):
        out = os.path.join(workdir, os.path.basename(path))
        try:
            import shutil
            shutil.copyfile(path, out)
            return out
        except OSError:
            return None
    out = os.path.join(workdir, os.path.basename(path) + ".jpg")
    result = subprocess.run(
        ["/usr/bin/sips", "-s", "format", "jpeg", path, "--out", out],
        capture_output=True,
        text=True,
        timeout=60,
    )
    return out if result.returncode == 0 else None


def wait_for_file(path: str, seconds: float = 20) -> bool:
    """附件行先落库、文件后下载完,等一会儿。"""
    deadline = time.time() + seconds
    while time.time() < deadline:
        if os.path.exists(path) and os.path.getsize(path) > 0:
            return True
        time.sleep(1)
    return os.path.exists(path)


MEME_ROOT = os.path.join(HOME, ".gqy", "data", "memes")
MEME_SENT_RE = re.compile(r"sent meme ([0-9a-f]{4,64})")
MAX_MEME_BYTES = 20 * 1024 * 1024


def resolve_meme(short_id: str):
    """按短 id 在本机表情包库里找图片文件。

    只认 MEME_ROOT 下各库 index.json 登记过的文件,且真实路径必须仍在该库
    目录内,防止 index 里的 `file` 字段被写成库外路径。匹配不唯一时放弃。
    """
    matches = []
    try:
        libraries = os.listdir(MEME_ROOT)
    except OSError:
        return None
    for library in libraries:
        lib_dir = os.path.realpath(os.path.join(MEME_ROOT, library))
        try:
            with open(os.path.join(lib_dir, "index.json"), encoding="utf-8") as f:
                items = json.load(f).get("memes", [])
        except (OSError, ValueError, AttributeError):
            continue
        for item in items:
            digest = str(item.get("id", "")).split(":")[-1]
            if digest.startswith(short_id) and item.get("file"):
                path = safe_library_file(lib_dir, item["file"])
                if path:
                    matches.append(path)
    matches = list(dict.fromkeys(matches))
    return matches[0] if len(matches) == 1 else None


ALBUM_GLOB_ROOT = os.path.join(HOME, ".gqy", "home")
ALBUM_SENT_RE = re.compile(r"^sent .* \(id ([0-9A-Za-z_-]{4,64})\)\s*$", re.S)
IMAGE_MAGIC = (
    b"\xff\xd8\xff",  # JPEG
    b"\x89PNG\r\n\x1a\n",
    b"GIF87a",
    b"GIF89a",
)


def is_image_file(path: str) -> bool:
    """按文件头判断是不是图片,不信扩展名。"""
    try:
        with open(path, "rb") as f:
            head = f.read(16)
    except OSError:
        return False
    if head.startswith(IMAGE_MAGIC):
        return True
    if head[:4] == b"RIFF" and head[8:12] == b"WEBP":
        return True
    # HEIC/HEIF:ftyp 盒子
    return head[4:8] == b"ftyp" and head[8:12] in (b"heic", b"heix", b"mif1", b"msf1")


def safe_library_file(lib_dir: str, relative: str):
    """库内相对路径 → 真实路径。必须仍在库目录内、是图片、不超过上限。"""
    lib_dir = os.path.realpath(lib_dir)
    path = os.path.realpath(os.path.join(lib_dir, relative))
    if not path.startswith(lib_dir + os.sep):
        return None
    if not os.path.isfile(path) or os.path.getsize(path) > MAX_MEME_BYTES:
        return None
    return path if is_image_file(path) else None


def resolve_album(entry_id: str):
    """按 id 在本机各人格图库里找图片。匹配不唯一时放弃。"""
    matches = []
    try:
        homes = os.listdir(ALBUM_GLOB_ROOT)
    except OSError:
        return None
    for home in homes:
        album_root = os.path.join(ALBUM_GLOB_ROOT, home, "pictures", "album")
        try:
            scopes = os.listdir(album_root)
        except OSError:
            continue
        for scope in scopes:
            lib_dir = os.path.join(album_root, scope)
            try:
                with open(os.path.join(lib_dir, "index.json"), encoding="utf-8") as f:
                    entries = json.load(f)
            except (OSError, ValueError):
                continue
            if not isinstance(entries, list):
                continue
            for entry in entries:
                if isinstance(entry, dict) and entry.get("id") == entry_id and entry.get("file"):
                    path = safe_library_file(lib_dir, entry["file"])
                    if path:
                        matches.append(path)
    matches = list(dict.fromkeys(matches))
    return matches[0] if len(matches) == 1 else None


def run_turn(config: Config, contact: str, content: str, images: list):
    """跑一个回合,返回 (最终正文, 本回合发出的表情包文件列表)。"""
    cmd = [
        config.gqy_bin,
        "ask",
        "--session",
        f"imessage-{contact}",
        "--create",
        "--output-format",
        "stream-json",
        "--timeout",
        str(config.timeout_seconds),
        "--cwd",
        HOME,
        "--tools",
        ",".join(config.tools),
        "--stdin",
    ]
    if os.path.exists(HINT_PATH):
        cmd += ["--append-system-prompt", "@" + HINT_PATH]
    for image in images:
        cmd += ["--image", image]

    for attempt in range(3):
        result = subprocess.run(
            cmd,
            input=content,
            capture_output=True,
            text=True,
            timeout=config.timeout_seconds + 60,
        )
        events = parse_events(result.stdout)
        final = events[-1] if events else None
        if result.returncode == 0 and final and final.get("type") == "done":
            return final.get("text") or "", collect_memes(events)
        # 只取错误类别与前 120 字,避免把回合内容带进日志
        message = ((final or {}).get("message") or result.stderr.strip())[:120]
        if "busy" in (message or "").lower() and attempt < 2:
            time.sleep(5)
            continue
        raise RuntimeError(f"gqy ask exit {result.returncode}: {message}")
    return "", []


def parse_events(output: str) -> list:
    events = []
    for line in output.splitlines():
        try:
            value = json.loads(line)
        except ValueError:
            continue
        if isinstance(value, dict):
            events.append(value)
    return events


def collect_memes(events: list) -> list:
    """本回合她「发出」的图:表情包(use_meme)与图库(album show)。

    只认工具成功输出里的 id,再到本机库里按 index 解析,不接受任意路径。
    """
    pictures = []
    for event in events:
        if not (event.get("type") == "tool" and event.get("phase") == "end" and event.get("ok")):
            continue
        name = event.get("name")
        output = str(event.get("output", "")).strip()
        if name == "use_meme":
            match = MEME_SENT_RE.search(output)
            resolve, kind = resolve_meme, "meme"
        elif name == "album":
            match = ALBUM_SENT_RE.match(output)
            resolve, kind = resolve_album, "album picture"
        else:
            continue
        if not match:
            continue
        path = resolve(match.group(1))
        if path:
            pictures.append(path)
        else:
            log.warning("%s %s not found in local library", kind, match.group(1))
    return pictures


class ContactWorker(threading.Thread):
    """每个联系人一条串行队列。回合进行中新到的消息攒到下一轮一起发。"""

    def __init__(self, contact: str, watcher: ConfigWatcher, watermark: Watermark) -> None:
        super().__init__(name=f"contact-{contact}", daemon=True)
        self.contact = contact
        self.watcher = watcher
        self.watermark = watermark
        self.inbox: queue.Queue = queue.Queue()

    def run(self) -> None:
        while True:
            batch = [self.inbox.get()]
            config = self.watcher.get()
            # 连发几条时等对方说完
            deadline = time.time() + config.batch_wait_seconds
            while True:
                remaining = deadline - time.time()
                if remaining <= 0:
                    break
                try:
                    batch.append(self.inbox.get(timeout=remaining))
                    deadline = time.time() + config.batch_wait_seconds
                except queue.Empty:
                    break
            try:
                self.handle_batch(config, batch)
            except Exception:  # noqa: BLE001 — 单轮失败不能拖死整条队列
                log.exception("turn for %s failed", self.contact)
            finally:
                self.watermark.release(m.rowid for m in batch)

    def handle_batch(self, config: Config, batch: list) -> None:
        reply_handle = batch[-1].handle
        with tempfile.TemporaryDirectory(prefix="gqy-imessage-") as workdir:
            lines, images = [], []
            for message in batch:
                if message.text:
                    lines.append(message.text)
                for path, mime, name in message.attachments:
                    if not wait_for_file(path):
                        lines.append(f"[attachment unavailable: {name}]")
                        continue
                    image = prepare_image(path, mime, workdir)
                    if image:
                        images.append(image)
                        if not message.text:
                            lines.append("[image]")
                    else:
                        lines.append(f"[attachment: {name}]")
            content = "\n".join(lines).strip() or "[image]"
            log.info(
                "turn start: contact=%s messages=%d chars=%d images=%d",
                self.contact,
                len(batch),
                len(content),
                len(images),
            )
            started = time.time()
            reply, memes = run_turn(config, self.contact, content, images)

        plain = markdown_to_plain(reply)
        if not plain and not memes:
            log.info("turn done with empty reply (%.1fs)", time.time() - started)
            return
        bubbles = []
        if plain:
            bubbles = (
                split_bubbles(plain, config.max_bubbles) if config.split_paragraphs else [plain]
            )
        conn = open_chat_db()
        try:
            before = max_rowid(conn)
        finally:
            conn.close()
        for bubble in bubbles:
            send_text(reply_handle, bubble)
        for meme in memes[: config.max_memes]:
            send_file(reply_handle, meme)
        log.info(
            "turn done: contact=%s bubbles=%d memes=%d chars=%d (%.1fs)",
            self.contact,
            len(bubbles),
            min(len(memes), config.max_memes),
            len(plain),
            time.time() - started,
        )
        check_delivery(reply_handle, before, len(bubbles) + min(len(memes), config.max_memes))


# ---------------------------------------------------------------------------
# 主循环
# ---------------------------------------------------------------------------


class SelfReloader:
    """脚本文件被改动且能编译通过时,空闲下来就用新代码替换本进程。"""

    def __init__(self) -> None:
        self.path = os.path.abspath(__file__)
        self.mtime = os.stat(self.path).st_mtime

    def changed(self) -> bool:
        try:
            mtime = os.stat(self.path).st_mtime
        except OSError:
            return False
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


DEBUG_REQUEST = os.path.join(HOME, ".gqy", "state", "imessage-debug-rowids.json")


def run_debug_request() -> None:
    """排查用:发现请求文件就把指定 ROWID 的消息与附件字段写进日志,然后删掉请求。

    只读、只查固定字段,不记录正文内容(只记长度)。请求文件格式 {"rowids": [1, 2]}。
    """
    try:
        with open(DEBUG_REQUEST, encoding="utf-8") as f:
            rowids = [int(r) for r in json.load(f).get("rowids", [])][:20]
    except (OSError, ValueError, TypeError, AttributeError):
        return
    finally:
        try:
            os.remove(DEBUG_REQUEST)
        except OSError:
            pass
    conn = open_chat_db()
    try:
        for rowid in rowids:
            m = conn.execute(
                "SELECT ROWID, guid, error, is_sent, is_delivered, is_finished, item_type, "
                "associated_message_type, balloon_bundle_id, cache_has_attachments, "
                "length(text) AS text_len, length(attributedBody) AS body_len, service, "
                "datetime(date/1000000000 + 978307200, 'unixepoch', 'localtime') AS sent_at, "
                "CASE WHEN date_delivered > 0 THEN datetime(date_delivered/1000000000 + 978307200, 'unixepoch', 'localtime') END AS delivered_at "
                "FROM message WHERE ROWID = ?",
                (rowid,),
            ).fetchone()
            log.info("debug message %s: %s", rowid, dict(m) if m else None)
            for a in conn.execute(
                "SELECT a.ROWID, a.transfer_state, a.total_bytes, a.mime_type, a.uti, "
                "a.is_outgoing, a.hide_attachment, a.filename "
                "FROM attachment a JOIN message_attachment_join j ON j.attachment_id = a.ROWID "
                "WHERE j.message_id = ?",
                (rowid,),
            ).fetchall():
                log.info("debug attachment of %s: %s", rowid, dict(a))
    finally:
        conn.close()


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


def initial_rowid(conn: sqlite3.Connection, config: Config) -> int:
    current = max_rowid(conn)
    saved = load_state().get("last_rowid")
    if saved is None or saved > current:
        return current
    # 停机太久时不回灌陈旧消息
    cutoff_ns = (time.time() - APPLE_EPOCH - config.max_backlog_minutes * 60) * 1e9
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
    handler.setFormatter(
        logging.Formatter("%(asctime)s %(levelname)s %(threadName)s %(message)s")
    )
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
    workers: dict = {}
    last_sig = None

    while True:
        config = watcher.get()
        sig = db_signature()
        if sig != last_sig:
            last_sig = sig
            try:
                top, inbound = fetch_new(conn, watermark.seen)
            except sqlite3.Error as error:
                log.warning("query failed, reopening db: %s", error)
                conn.close()
                time.sleep(config.poll_seconds)
                conn = open_chat_db()
                last_sig = None
                continue
            for message in inbound:
                contact = config.handle_to_contact.get(message.handle)
                if not config.enabled or contact is None:
                    continue
                watermark.claim(message.rowid)
                worker = workers.get(contact)
                if worker is None:
                    worker = ContactWorker(contact, watcher, watermark)
                    worker.start()
                    workers[contact] = worker
                worker.inbox.put(message)
            watermark.seen = top
            watermark.flush()

        run_debug_request()
        if watermark.idle() and reloader.changed():
            conn.close()
            reloader.exec()
        time.sleep(config.poll_seconds)


if __name__ == "__main__":
    sys.exit(main())
