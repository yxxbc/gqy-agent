"""iMessage 里的快捷指令：话题（会话）切换、模型切换、暂停。

指令由桥接直接处理、直接回复，不进模型回合，也不写进会话记录。只认下面列出的
指令名，普通消息里恰好以 / 开头（比如发一个路径）不受影响。

每个联系人的偏好（当前话题、模型、是否暂停）存在 ~/.gqy/state/imessage-contacts.json，
和处理水位分开：水位文件每次都整体覆盖写。
"""

import json
import os
import re
import subprocess
import threading
from datetime import datetime, timezone

HOME = os.path.expanduser("~")
PREFS_PATH = os.path.join(HOME, ".gqy", "state", "imessage-contacts.json")
_lock = threading.Lock()

HELP = """可用指令：
/new 开一个新话题（旧话题都保留）
/topics 看所有话题
/topic 2 切换到第 2 个话题
/model 看当前模型和可选模型
/model 5 或 /model 名字 切换模型，/model default 恢复默认
/pause 暂停回复，/resume 恢复
/help 显示这段说明"""

COMMANDS = {"help", "new", "topics", "topic", "model", "pause", "resume"}


def parse_command(text: str):
    """是指令就返回 (指令名, 参数)，否则 None。"""
    match = re.fullmatch(r"\s*/([a-zA-Z]+)(?:\s+(.*?))?\s*", text or "", re.S)
    if not match or match.group(1).lower() not in COMMANDS:
        return None
    return match.group(1).lower(), (match.group(2) or "").strip()


# ---------------------------------------------------------------------------
# 偏好
# ---------------------------------------------------------------------------


def _load_all() -> dict:
    try:
        with open(PREFS_PATH, encoding="utf-8") as f:
            data = json.load(f)
        return data if isinstance(data, dict) else {}
    except (FileNotFoundError, ValueError):
        return {}


def prefs(contact: str) -> dict:
    """{"topic": int, "model": str | None, "paused": bool}，缺省是话题 1、默认模型、不暂停。"""
    with _lock:
        stored = _load_all().get(contact) or {}
    return {
        "topic": max(1, int(stored.get("topic") or 1)),
        "model": stored.get("model") or None,
        "paused": bool(stored.get("paused")),
    }


def update_prefs(contact: str, **changes) -> None:
    with _lock:
        data = _load_all()
        entry = data.setdefault(contact, {})
        entry.update(changes)
        os.makedirs(os.path.dirname(PREFS_PATH), exist_ok=True)
        tmp = PREFS_PATH + ".tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump(data, f, ensure_ascii=False)
        os.replace(tmp, PREFS_PATH)


def session_name(contact: str, topic: int) -> str:
    """话题 1 沿用原来的会话名，老对话不断档。"""
    return f"imessage-{contact}" if topic <= 1 else f"imessage-{contact}-{topic}"


# ---------------------------------------------------------------------------
# 从 gqy 读会话与模型
# ---------------------------------------------------------------------------


def _gqy(gqy_bin: str, *args: str) -> str:
    result = subprocess.run([gqy_bin, *args], capture_output=True, text=True, timeout=30)
    if result.returncode != 0:
        raise RuntimeError((result.stderr or result.stdout).strip()[:120])
    return result.stdout


def list_topics(gqy_bin: str, contact: str) -> dict:
    """{话题号: 会话信息}，只含已经聊过的话题。"""
    data = json.loads(_gqy(gqy_bin, "session", "list", "--json"))
    sessions = data.get("sessions", data) if isinstance(data, dict) else data
    base = f"imessage-{contact}"
    topics = {}
    for session in sessions:
        name = session.get("name") or ""
        if name == base:
            topics[1] = session
        elif (match := re.fullmatch(re.escape(base) + r"-(\d+)", name)) and int(match.group(1)) > 1:
            topics[int(match.group(1))] = session
    return topics


def list_models(gqy_bin: str) -> list:
    """[(序号, 供应商, 模型名)]，序号与 `gqy list-models` 一致，只用于这一次挑选。"""
    models = []
    for line in _gqy(gqy_bin, "list-models").splitlines():
        match = re.match(r"\s*\[.\]\s*(\d+)\.\s*(.+?)\s*/\s*(.+?)\s*$", line)
        if match:
            models.append((int(match.group(1)), match.group(2), match.group(3)))
    return models


def _ago(stamp: str) -> str:
    try:
        then = datetime.fromisoformat(stamp.replace("Z", "+00:00"))
    except (TypeError, ValueError):
        return ""
    minutes = int((datetime.now(timezone.utc) - then).total_seconds() // 60)
    if minutes < 60:
        return f"{max(minutes, 0)} 分钟前"
    if minutes < 60 * 24:
        return f"{minutes // 60} 小时前"
    return f"{minutes // (60 * 24)} 天前"


def _snippet(text: str, limit: int = 18) -> str:
    text = " ".join(str(text or "").split())
    return text if len(text) <= limit else text[:limit] + "…"


# ---------------------------------------------------------------------------
# 执行
# ---------------------------------------------------------------------------


def run_command(name: str, arg: str, contact: str, gqy_bin: str) -> str:
    """执行指令，返回要回给对方的一段纯文本。"""
    current = prefs(contact)
    if name == "help":
        return HELP
    if name == "pause":
        update_prefs(contact, paused=True)
        return "已暂停。这期间的消息我不会回复，发 /resume 恢复。"
    if name == "resume":
        update_prefs(contact, paused=False)
        return "已恢复，接着聊吧。"
    if name == "new":
        topics = list_topics(gqy_bin, contact)
        number = max([current["topic"], *topics.keys()]) + 1
        update_prefs(contact, topic=number)
        return f"好，开始新话题（话题 {number}）。之前的话题都还在，发 /topics 查看。"
    if name == "topics":
        topics = list_topics(gqy_bin, contact)
        if not topics:
            return "还没有聊过的话题。"
        lines = []
        for number in sorted(topics):
            session = topics[number]
            mark = "▶ " if number == current["topic"] else ""
            detail = "，".join(
                part
                for part in (
                    f"{session.get('turn_count', 0)} 轮",
                    _ago(session.get("updated_at", "")),
                    _snippet(session.get("last_user_content")),
                )
                if part
            )
            lines.append(f"{mark}{number}. {detail}")
        if current["topic"] not in topics:
            lines.append(f"▶ {current['topic']}. 新话题，还没开始聊")
        return "\n".join(lines) + "\n\n发 /topic 序号 切换。"
    if name == "topic":
        if not arg.isdigit():
            return "要切到哪个话题？比如 /topic 2。发 /topics 看列表。"
        number = int(arg)
        topics = list_topics(gqy_bin, contact)
        if number not in topics and number != current["topic"]:
            return f"没有话题 {number}。发 /topics 看列表，或者 /new 开新话题。"
        update_prefs(contact, topic=number)
        return f"已切到话题 {number}。"
    if name == "model":
        return _model_command(arg, contact, gqy_bin, current)
    return HELP


def _model_command(arg: str, contact: str, gqy_bin: str, current: dict) -> str:
    models = list_models(gqy_bin)
    if arg.lower() in ("default", "默认"):
        update_prefs(contact, model=None)
        return "已恢复默认模型。"
    if not arg:
        now = current["model"] or "默认（跟随全局设置）"
        lines, provider = [f"当前模型：{now}", ""], None
        for number, vendor, model in models:
            if vendor != provider:
                lines.append(f"【{vendor}】")
                provider = vendor
            lines.append(f"{number}. {model}")
        return "\n".join(lines) + "\n\n发 /model 序号 或 /model 名字 切换。"
    if arg.isdigit():
        picked = [m for m in models if m[0] == int(arg)]
    else:
        needle = arg.lower()
        exact = [m for m in models if m[2].lower() == needle]
        picked = exact or [m for m in models if needle in m[2].lower()]
    if not picked:
        return f"没找到「{arg}」。发 /model 看可选列表。"
    if len(picked) > 1:
        return "匹配到多个：\n" + "\n".join(f"{n}. {v} / {m}" for n, v, m in picked[:8]) + "\n\n用序号选一个。"
    _, vendor, model = picked[0]
    # 记裸名而不是序号：序号会随供应商配置变化
    update_prefs(contact, model=model)
    return f"已切换到 {vendor} / {model}，只对这个聊天生效。"
