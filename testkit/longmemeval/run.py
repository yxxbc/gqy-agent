#!/usr/bin/env python3
"""LongMemEval（ICLR 2025）跑分：顾清影的长期记忆能答对多少。

每道题一个全新沙箱：把这道题的「历史会话」按原日期写成她的短期日记（与
`MemoryStore::process_after_turn` 同一格式：`时间，对方说：…；我回：…`，
对方截 260 字、她截 520 字——这就是她真实记住的样子），可选让整理器把日记
整理成事实，然后用一次性回合提问（只给记忆召回工具，告诉她「今天」是题目日期），
最后另开一个干净沙箱当裁判，按 LongMemEval 官方的分题型判分提示判对错。

会花真实模型的额度：答题一回合 + 裁判一回合；CONSOLIDATE=1 时整理器每题还要跑
几十批（大头）。先用小样本估费：

    N=30 TAG=s30 python3 testkit/longmemeval/run.py
    N=3 CONSOLIDATE=1 TAG=s3-consolidated python3 testkit/longmemeval/run.py

环境变量：
    BIN          gqy 可执行文件（默认 ~/.cargo/bin/gqy）
    SRC_HOME     只从这里拷供应商配置（默认 ~/.gqy）；不拷记忆、会话、平台、语音
    DATA         longmemeval_s_cleaned.json（默认 ~/.cache/gqy-longmemeval/，缺了自动下载）
    N / SEED     抽多少题 / 抽样种子（按题型轮流抽，题型分布尽量均匀）
    ONLY         逗号分隔的 question_id，指定就只跑这些
    CONSOLIDATE  1 = 提问前先让整理器把日记整理完（等 ORGANIZER_WAIT 秒为上限）
    MODEL        答题回合的模型（provider/model），默认用配置里的
    JUDGE_MODEL  裁判的模型，默认同 MODEL
    OUT / TAG    结果目录（默认 ~/.cache/gqy-longmemeval/<TAG>）
    TIMEOUT      单次回合超时秒数（默认 300）

判分提示改写自 LongMemEval 的 src/evaluation/evaluate_qa.py。官方裁判是 gpt-4o，
这里用配置里的模型，分数只能和同一裁判下的结果直接比。
"""

import collections
import json
import os
import random
import shutil
import sqlite3
import subprocess
import sys
import time
import urllib.request
from datetime import datetime, timedelta, timezone
from pathlib import Path

DATA_URL = "https://huggingface.co/datasets/xiaowu0162/longmemeval-cleaned/resolve/main/longmemeval_s_cleaned.json"
CACHE = Path("~/.cache/gqy-longmemeval").expanduser()
BIN = Path(os.environ.get("BIN", "~/.cargo/bin/gqy")).expanduser()
SRC_HOME = Path(os.environ.get("SRC_HOME", "~/.gqy")).expanduser()
DATA = Path(os.environ.get("DATA", CACHE / "longmemeval_s_cleaned.json")).expanduser()
N = int(os.environ.get("N", "30"))
SEED = int(os.environ.get("SEED", "7"))
ONLY = [item for item in os.environ.get("ONLY", "").split(",") if item]
CONSOLIDATE = os.environ.get("CONSOLIDATE", "0") == "1"
ORGANIZER_WAIT = int(os.environ.get("ORGANIZER_WAIT", "1800"))
MODEL = os.environ.get("MODEL", "")
JUDGE_MODEL = os.environ.get("JUDGE_MODEL", MODEL)
TAG = os.environ.get("TAG", "run")
OUT = Path(os.environ.get("OUT", CACHE / TAG)).expanduser()
TIMEOUT = int(os.environ.get("TIMEOUT", "300"))
BASE_PORT = int(os.environ.get("PORT", "18530"))

# 与 src/memory/association.rs::diary_content、search.rs::truncate_chars 保持一致。
USER_CHARS, ASSISTANT_CHARS = 260, 520

ANSWER_CONTEXT = (
    "<benchmark-context>Today is {date}. Answer the question from what you remember of your earlier "
    "conversations with this person; use the memory recall tool when your recollection is incomplete. "
    "If you have no memory of it, say you don't know instead of guessing.</benchmark-context>"
)

JUDGE_SYSTEM = "You grade answers for a memory benchmark. Reply with exactly one word: yes or no."
JUDGE_DEFAULT = (
    "I will give you a question, a correct answer, and a response from a model. Please answer yes if the "
    "response contains the correct answer. Otherwise, answer no. If the response is equivalent to the correct "
    "answer or contains all the intermediate steps to get the correct answer, you should also answer yes. If the "
    "response only contains a subset of the information required by the answer, answer no.{extra}\n\n"
    "Question: {question}\n\nCorrect Answer: {answer}\n\nModel Response: {response}\n\n"
    "Is the model response correct? Answer yes or no only."
)
JUDGE_EXTRA = {
    "temporal-reasoning": " In addition, do not penalize off-by-one errors for the number of days. If the question "
    "asks for the number of days/weeks/months, etc., and the model makes off-by-one errors (e.g., predicting 19 "
    "days when the answer is 18), the model's response is still correct.",
    "knowledge-update": " If the response contains some previous information along with an updated answer, the "
    "response should be considered as correct as long as the updated answer is the required answer.",
}
JUDGE_PREFERENCE = (
    "I will give you a question, a rubric for desired personalized response, and a response from a model. Please "
    "answer yes if the response satisfies the desired response. Otherwise, answer no. The model does not need to "
    "reflect all the points in the rubric. The response is correct as long as it recalls and utilizes the user's "
    "personal information correctly.\n\nQuestion: {question}\n\nRubric: {answer}\n\nModel Response: {response}\n\n"
    "Is the model response correct? Answer yes or no only."
)
JUDGE_ABSTENTION = (
    "I will give you an unanswerable question, an explanation, and a response from a model. Please answer yes if "
    "the model correctly identifies the question as unanswerable. The model could say that the information is "
    "incomplete, or some other information is given but the asked information is not.\n\nQuestion: {question}\n\n"
    "Explanation: {answer}\n\nModel Response: {response}\n\nDoes the model correctly identify the question as "
    "unanswerable? Answer yes or no only."
)


def load_data():
    if not DATA.exists():
        DATA.parent.mkdir(parents=True, exist_ok=True)
        print(f"· 下载 {DATA_URL}", flush=True)
        urllib.request.urlretrieve(DATA_URL, DATA)
    return json.loads(DATA.read_text())


def category(item):
    return "abstention" if item["question_id"].endswith("_abs") else item["question_type"]


def sample(items):
    if ONLY:
        wanted = set(ONLY)
        return [item for item in items if item["question_id"] in wanted]
    rng = random.Random(SEED)
    buckets = collections.defaultdict(list)
    for item in items:
        buckets[category(item)].append(item)
    for bucket in buckets.values():
        rng.shuffle(bucket)
    picked, order = [], sorted(buckets)
    while len(picked) < N and any(buckets.values()):
        for name in order:
            if buckets[name] and len(picked) < N:
                picked.append(buckets[name].pop())
    return picked


def compact(text):
    return " ".join(str(text).split())


def truncate(text, limit):
    return text if len(text) <= limit else text[: max(limit - 3, 0)] + "..."


def parse_date(text):
    """'2023/05/20 (Sat) 02:21' → 本机时区的 datetime。"""
    stamp = datetime.strptime(text.split(" (")[0] + " " + text.split(") ")[-1], "%Y/%m/%d %H:%M")
    return stamp.astimezone()


def sandbox_config(src, dst, memory_enabled):
    """只带供应商、模型池与记忆配置进沙箱：平台、语音、MCP、通知、技能一律不带。"""
    config = json.loads(src.read_text())
    for key in ("platforms", "voice", "mcp", "notifications", "system_prompt_file", "skills"):
        config.pop(key, None)
    config.setdefault("tools", {})["enabled"] = True
    memory = config.setdefault("memory", {})
    memory["enabled"] = memory_enabled
    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.write_text(json.dumps(config, ensure_ascii=False, indent=2))


class Sandbox:
    def __init__(self, root, port, memory_enabled):
        self.home = root / "home"
        self.runtime = root / "run"
        self.log = root / "daemon.log"
        self.port = port
        if root.exists():
            shutil.rmtree(root)
        self.runtime.mkdir(parents=True)
        sandbox_config(SRC_HOME / "config" / "config.jsonc", self.home / "config" / "config.jsonc", memory_enabled)
        self.env = dict(os.environ, GQY_HOME=str(self.home), XDG_RUNTIME_DIR=str(self.runtime))
        self.env.pop("GQY_WEB_DIR", None)

    def run(self, args, timeout=TIMEOUT, stdin=None):
        return subprocess.run([str(BIN), *args], env=self.env, cwd=str(self.home), input=stdin,
                              capture_output=True, text=True, timeout=timeout)

    def memory_db(self):
        found = sorted(self.home.glob("personas/*/memory/memory.db")) or sorted(
            self.home.glob("data/personas/*/memory/memory.db"))
        return found[0] if found else None

    def ledger(self):
        """daemon 自己的用量账本：每回合一行（含该回合所有工具轮的累计）。
        流式输出里的 usage 事件不全，以这里为准。"""
        path = self.home / "state" / "usage-history.jsonl"
        total, models = collections.Counter(), collections.Counter()
        if path.exists():
            for line in path.read_text().splitlines():
                row = json.loads(line)
                for key in ("prompt", "completion", "total"):
                    total[key] += row.get(key, 0)
                models[f"{row.get('provider')}/{row.get('model')}"] += 1
        return {"tokens": dict(total), "models": dict(models)}

    def stop(self):
        self.run(["daemon", "stop"], timeout=60)


def turn(sandbox, prompt, extra_args):
    """一次性回合，stream-json 逐事件读：累加每次模型请求的用量、数记忆工具调用。"""
    args = ["--output-format", "stream-json", "--no-memory", *extra_args]
    started = time.time()
    result = sandbox.run([*args, prompt])
    usage = collections.Counter()
    text, tools, error = "", collections.Counter(), None
    for line in result.stdout.splitlines():
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        kind = event.get("type")
        if kind == "usage" and isinstance(event.get("usage"), dict):
            for key, value in event["usage"].items():
                if isinstance(value, (int, float)):
                    usage[key] += value
        elif kind == "tool" and event.get("phase") in ("start", "started", "call"):
            tools[event.get("name", "?")] += 1
        elif kind == "done":
            text = event.get("text", "")
        elif kind == "error":
            error = event.get("message")
    if not text and error is None:
        error = (result.stderr or "no done event").strip()[-400:]
    return {"text": text, "usage": dict(usage), "tools": dict(tools), "error": error,
            "seconds": round(time.time() - started, 1)}


def seed_diaries(db, item):
    """历史会话按原日期写成短期日记；同一会话内的轮次按秒递增保持顺序。"""
    con = sqlite3.connect(db)
    count = 0
    for session_id, date, session in zip(item["haystack_session_ids"], item["haystack_dates"], item["haystack_sessions"]):
        base = parse_date(date)
        pending_user = None
        pairs = []
        for message in session:
            if message["role"] == "user":
                if pending_user is not None:
                    pairs.append((pending_user, ""))
                pending_user = message["content"]
            else:
                pairs.append((pending_user or "", message["content"]))
                pending_user = None
        if pending_user is not None:
            pairs.append((pending_user, ""))
        for index, (user, assistant) in enumerate(pairs):
            created = (base + timedelta(seconds=index)).astimezone(timezone.utc).isoformat()
            content = f"{created}，对方说：{truncate(compact(user), USER_CHARS)}；我回：{truncate(compact(assistant), ASSISTANT_CHARS)}"
            con.execute(
                "INSERT INTO episodes (content, source, status, strength, recall_count, created_at, updated_at,"
                " retention, user_message, assistant_message, expires_at, origin_kind, origin_session_id, visibility)"
                " VALUES (?, 'episode', 'active', 1.0, 0, ?, ?, 'short_term', ?, ?, NULL, 'local', ?, 'privileged')",
                (content, created, created, user.strip(), assistant.strip(), session_id),
            )
            count += 1
    con.commit()
    con.close()
    return count


def db_count(db, sql):
    con = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
    try:
        return con.execute(sql).fetchone()[0]
    except sqlite3.OperationalError:
        return None
    finally:
        con.close()


def consolidate(sandbox, db):
    daemon = subprocess.Popen([str(BIN), "__daemon", "--port", str(sandbox.port)], env=sandbox.env,
                              cwd=str(sandbox.home), stdout=sandbox.log.open("w"), stderr=subprocess.STDOUT)
    started = time.time()
    try:
        while time.time() - started < ORGANIZER_WAIT:
            time.sleep(15)
            pending = db_count(db, "SELECT count(*) FROM episodes WHERE retention='short_term' AND consolidated_at IS NULL")
            if pending == 0:
                break
    finally:
        daemon.terminate()
        try:
            daemon.wait(timeout=15)
        except subprocess.TimeoutExpired:
            daemon.kill()
    return round(time.time() - started)


def warm_embeddings(sandbox, db):
    """开了语义检索时，直接写库的日记没有向量；召回一次会触发后台补建，等它补完。"""
    sandbox.run(["tool-call", "recall_memories", json.dumps({"query": "warm up"})], timeout=120)
    total = db_count(db, "SELECT count(*) FROM episodes") or 0
    last, stable = -1, 0
    for _ in range(40):
        done = db_count(db, "SELECT count(*) FROM memory_embeddings")
        if not done:
            return 0
        if done >= total or done == last:
            stable += 1
            if done >= total or stable >= 3:
                return done
        last = done
        time.sleep(5)
    return last


def judge(judge_box, item, response):
    kind = category(item)
    fields = {"question": item["question"], "answer": item["answer"], "response": response}
    if kind == "abstention":
        prompt = JUDGE_ABSTENTION.format(**fields)
    elif kind == "single-session-preference":
        prompt = JUDGE_PREFERENCE.format(**fields)
    else:
        prompt = JUDGE_DEFAULT.format(extra=JUDGE_EXTRA.get(kind, ""), **fields)
    args = ["--no-tools", "--system-prompt", JUDGE_SYSTEM]
    if JUDGE_MODEL:
        args += ["--model", JUDGE_MODEL]
    verdict = turn(judge_box, prompt, args)
    return verdict["text"].strip().lower().startswith("yes"), verdict


def main():
    if not BIN.exists():
        sys.exit(f"找不到 {BIN}")
    items = sample(load_data())
    OUT.mkdir(parents=True, exist_ok=True)
    results_path = OUT / "results.jsonl"
    done = {}
    if results_path.exists():
        for line in results_path.read_text().splitlines():
            row = json.loads(line)
            done[row["question_id"]] = row
    print(f"· {len(items)} 题，已完成 {len(done)}；CONSOLIDATE={int(CONSOLIDATE)}；结果 {OUT}", flush=True)
    judge_box = Sandbox(OUT / "judge", BASE_PORT + 1, memory_enabled=False)
    try:
        for index, item in enumerate(items, 1):
            qid = item["question_id"]
            if qid in done:
                continue
            box = Sandbox(OUT / "questions" / qid, BASE_PORT + 2 + index, memory_enabled=True)
            try:
                # 记忆库懒建：只读召回不会建库，用一次写工具建出来再把这条种子删掉。
                box.run(["tool-call", "remember_fact", json.dumps({"content": "benchmark init"})], timeout=120)
                db = box.memory_db()
                if db is None:
                    raise RuntimeError("memory.db was not created in the sandbox")
                con = sqlite3.connect(db)
                con.execute("DELETE FROM facts WHERE content='benchmark init'")
                con.commit()
                con.close()
                diaries = seed_diaries(db, item)
                box.stop()
                organizer_seconds = consolidate(box, db) if CONSOLIDATE else 0
                embedded = warm_embeddings(box, db)
                args = ["--tools", "recall_memories",
                        "--append-system-prompt", ANSWER_CONTEXT.format(date=item["question_date"])]
                if MODEL:
                    args += ["--model", MODEL]
                answer = turn(box, item["question"], args)
                correct, verdict = judge(judge_box, item, answer["text"]) if not answer["error"] else (False, None)
                row = {
                    "question_id": qid, "category": category(item), "question": item["question"],
                    "answer": item["answer"], "question_date": item["question_date"],
                    "response": answer["text"], "error": answer["error"], "correct": correct,
                    "diaries": diaries, "embedded": embedded, "organizer_seconds": organizer_seconds,
                    "facts": db_count(db, "SELECT count(*) FROM facts"),
                    "answer_usage": box.ledger(), "answer_tools": answer["tools"],
                    "answer_seconds": answer["seconds"],

                    "judge_text": verdict["text"] if verdict else "",
                }
            except Exception as error:  # 单题失败不拖垮整轮，记下来继续
                row = {"question_id": qid, "category": category(item), "error": repr(error), "correct": False}
            finally:
                box.stop()
            with results_path.open("a") as handle:
                handle.write(json.dumps(row, ensure_ascii=False) + "\n")
            done[qid] = row
            mark = "✓" if row.get("correct") else ("!" if row.get("error") else "✗")
            print(f"  [{index}/{len(items)}] {mark} {row['category']:<26} {qid}", flush=True)
    finally:
        judge_box.stop()
    summarize(list(done.values()), judge_box.ledger())


def summarize(rows, judge_usage):
    by = collections.defaultdict(list)
    for row in rows:
        by[row["category"]].append(row)
    usage, models = collections.Counter(), collections.Counter()
    for row in rows:
        ledger = row.get("answer_usage") or {}
        for name, value in (ledger.get("tokens") or {}).items():
            usage[f"答题 {name}"] += value
        for name, value in (ledger.get("models") or {}).items():
            models[name] += value
    for name, value in (judge_usage.get("tokens") or {}).items():
        usage[f"裁判 {name}"] += value
    answered = sum(1 for row in rows if (row.get("answer_usage") or {}).get("tokens"))
    per_question = usage["答题 total"] / answered if answered else 0
    lines = [f"# LongMemEval_S · {TAG}", "", f"- 题数 {len(rows)}，整理器 {'开' if CONSOLIDATE else '关'}",
             f"- 总正确率 {sum(r.get('correct', False) for r in rows)}/{len(rows)}", "",
             "| 题型 | 对/总 |", "|---|---|"]
    for name in sorted(by):
        group = by[name]
        lines.append(f"| {name} | {sum(r.get('correct', False) for r in group)}/{len(group)} |")
    errors = [r for r in rows if r.get("error")]
    lines += ["", f"- 出错 {len(errors)} 题", "", "## 用量（daemon 账本，token）", ""]
    lines += [f"- {name}: {value:,}" for name, value in sorted(usage.items())]
    lines += [f"- 平均每题答题 {per_question:,.0f} token", f"- 答题模型：{dict(models)}"]
    report = "\n".join(lines) + "\n"
    (OUT / "summary.md").write_text(report)
    print(report)


if __name__ == "__main__":
    main()
