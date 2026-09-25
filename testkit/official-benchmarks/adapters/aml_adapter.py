"""Official AML (Agent Memory Leaderboard / LongMemEval) Benchmark Adapter.

Supports:
- Multi-session conversation history parsing
- Automated seeding into gqy-agent episodic SQLite memory database
- Temporal reasoning anchor injection (<benchmark-context>)
- Memory category classification (temporal-reasoning, knowledge-update, preference, recall, abstention)
- Official memory leaderboard submission export
"""

import collections
import json
import os
import random
import re
import sqlite3
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional

from core.models import BenchmarkPrediction, BenchmarkTask, EvaluationResult
from core.normalizer import AnswerNormalizer
from .base import BaseBenchmarkAdapter


USER_CHARS_LIMIT = 260
ASSISTANT_CHARS_LIMIT = 520


def compact_text(text: Any) -> str:
    return " ".join(str(text).split())


def truncate_text(text: str, limit: int) -> str:
    return text if len(text) <= limit else text[: max(limit - 3, 0)] + "..."


def parse_session_date(text: str) -> datetime:
    """Parse '2023/05/20 (Sat) 02:21' or ISO format into timezone-aware datetime."""
    text = text.strip()
    if " (" in text and ") " in text:
        dt_str = text.split(" (")[0] + " " + text.split(") ")[-1]
        stamp = datetime.strptime(dt_str, "%Y/%m/%d %H:%M")
        return stamp.astimezone()
    try:
        return datetime.fromisoformat(text).astimezone()
    except Exception:
        # Fallback to current utc
        return datetime.now(timezone.utc)


class AmlBenchmarkAdapter(BaseBenchmarkAdapter):
    """Adapter for Agent Memory Leaderboards (LongMemEval and related memory benchmarks)."""

    ANSWER_CONTEXT_TEMPLATE = (
        "<benchmark-context>Today is {date}. Answer the question from what you remember of your earlier "
        "conversations with this person; use the memory recall tool when your recollection is incomplete. "
        "If you have no memory of it, say you don't know instead of guessing.</benchmark-context>\n\n"
    )

    AML_SYSTEM_PROMPT = (
        "You are an assistant with persistent episodic memory evaluated on the Agent Memory Leaderboard.\n"
        "Recall relevant information from your memory database of past interactions to answer accurately.\n"
        "State your concise answer clearly. If the asked memory does not exist or was never mentioned, "
        "explicitly state that you do not have record of it."
    )

    @property
    def name(self) -> str:
        return "aml"

    def load_tasks(
        self,
        data_source: Optional[str] = None,
        split: str = "validation",
        limit: Optional[int] = None,
        seed: int = 42,
        filters: Optional[Dict[str, Any]] = None,
    ) -> List[BenchmarkTask]:
        """Load memory benchmark tasks from json / jsonl / cleaned LongMemEval dataset."""
        raw_items: List[Dict[str, Any]] = []

        if data_source and Path(data_source).exists():
            path = Path(data_source)
            if path.suffix == ".jsonl":
                with open(path, "r", encoding="utf-8") as f:
                    for line in f:
                        if line.strip():
                            raw_items.append(json.loads(line))
            elif path.suffix == ".json":
                with open(path, "r", encoding="utf-8") as f:
                    content = json.load(f)
                    raw_items = content if isinstance(content, list) else [content]
        else:
            # Fallback to bundled sample tasks
            sample_file = Path(__file__).resolve().parent.parent / "samples" / "aml_sample.jsonl"
            if sample_file.exists():
                with open(sample_file, "r", encoding="utf-8") as f:
                    for line in f:
                        if line.strip():
                            raw_items.append(json.loads(line))

        tasks: List[BenchmarkTask] = []
        for idx, item in enumerate(raw_items):
            task_id = str(item.get("question_id") or item.get("task_id") or f"aml_mem_{idx:03d}")
            question = item.get("question") or item.get("Question") or ""
            ground_truth = item.get("answer") or item.get("ground_truth") or item.get("rubric") or ""
            
            # Determine question type
            q_type = "abstention" if task_id.endswith("_abs") else item.get("question_type", "recall")
            
            # Memory sessions
            haystack_sessions = item.get("haystack_sessions", [])
            haystack_dates = item.get("haystack_dates", [])
            haystack_session_ids = item.get("haystack_session_ids", [])
            anchor_date = item.get("question_date") or item.get("anchor_date") or "2024-01-01"

            # Apply filters
            if filters:
                if "category" in filters and q_type != filters["category"]:
                    continue
                if "task_id" in filters:
                    allowed = filters["task_id"]
                    if isinstance(allowed, list) and task_id not in allowed:
                        continue
                    elif isinstance(allowed, str) and task_id != allowed:
                        continue

            task = BenchmarkTask(
                task_id=task_id,
                question=question,
                benchmark="aml",
                split=split,
                ground_truth=str(ground_truth) if ground_truth else None,
                metadata={"question_type": q_type, "anchor_date": anchor_date},
                extra={
                    "haystack_sessions": haystack_sessions,
                    "haystack_dates": haystack_dates,
                    "haystack_session_ids": haystack_session_ids,
                    "question_type": q_type,
                    "anchor_date": anchor_date,
                }
            )
            tasks.append(task)

        # Sampling if limit specified
        if limit is not None and limit < len(tasks):
            rng = random.Random(seed)
            buckets = collections.defaultdict(list)
            for t in tasks:
                cat = t.extra.get("question_type", "general")
                buckets[cat].append(t)
            for b in buckets.values():
                rng.shuffle(b)
            sampled: List[BenchmarkTask] = []
            keys = sorted(buckets.keys())
            while len(sampled) < limit and any(buckets.values()):
                for k in keys:
                    if buckets[k] and len(sampled) < limit:
                        sampled.append(buckets[k].pop())
            tasks = sampled

        return tasks

    def seed_task_memory(self, db_path: Path, task: BenchmarkTask):
        """Seed conversation history into SQLite memory database under episodes table."""
        sessions = task.extra.get("haystack_sessions", [])
        dates = task.extra.get("haystack_dates", [])
        session_ids = task.extra.get("haystack_session_ids", [])

        if not sessions:
            return

        con = sqlite3.connect(db_path)
        # Ensure schema
        con.execute(
            """
            CREATE TABLE IF NOT EXISTS episodes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                content TEXT NOT NULL,
                source TEXT NOT NULL,
                status TEXT NOT NULL,
                strength REAL NOT NULL,
                recall_count INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                retention TEXT NOT NULL,
                user_message TEXT,
                assistant_message TEXT,
                expires_at TEXT,
                origin_kind TEXT,
                origin_session_id TEXT,
                visibility TEXT
            )
            """
        )
        con.execute("DELETE FROM episodes")
        try:
            con.execute("DELETE FROM facts")
        except Exception:
            pass

        for s_idx, session in enumerate(sessions):
            s_date_str = dates[s_idx] if s_idx < len(dates) else "2023-01-01 12:00"
            s_id = session_ids[s_idx] if s_idx < len(session_ids) else f"sess_{s_idx}"
            base_date = parse_session_date(s_date_str)

            # Pair user and assistant messages
            pending_user = None
            pairs = []
            for msg in session:
                role = msg.get("role")
                content = msg.get("content", "")
                if role == "user":
                    if pending_user is not None:
                        pairs.append((pending_user, ""))
                    pending_user = content
                else:
                    pairs.append((pending_user or "", content))
                    pending_user = None
            if pending_user is not None:
                pairs.append((pending_user, ""))

            for t_idx, (user_msg, asst_msg) in enumerate(pairs):
                created = (base_date + timedelta(seconds=t_idx)).astimezone(timezone.utc).isoformat()
                u_trunc = truncate_text(compact_text(user_msg), USER_CHARS_LIMIT)
                a_trunc = truncate_text(compact_text(asst_msg), ASSISTANT_CHARS_LIMIT)
                content = f"{created}，对方说：{u_trunc}；我回：{a_trunc}"

                con.execute(
                    """
                    INSERT INTO episodes (
                        content, source, status, strength, recall_count, created_at, updated_at,
                        retention, user_message, assistant_message, expires_at, origin_kind,
                        origin_session_id, visibility
                    ) VALUES (?, 'episode', 'active', 1.0, 0, ?, ?, 'short_term', ?, ?, NULL, 'local', ?, 'privileged')
                    """,
                    (content, created, created, user_msg, asst_msg, str(s_id)),
                )

        con.commit()
        con.close()

    def prepare_task_environment(
        self,
        task: BenchmarkTask,
        sandbox: Any,
        task_dir: Path,
    ) -> Dict[str, Any]:
        """Prepare task sandbox and seed task memory episodes into the isolated DB."""
        task_dir = Path(task_dir).resolve()
        task_dir.mkdir(parents=True, exist_ok=True)

        # Seed memory DB in sandbox
        if hasattr(sandbox, "get_memory_db_path"):
            db_path = sandbox.get_memory_db_path()
            sandbox.init_memory_schema(db_path)
            self.seed_task_memory(db_path, task)

        return {
            "cwd": task_dir,
            "append_system_prompt": self.AML_SYSTEM_PROMPT,
        }

    def build_prompt(self, task: BenchmarkTask) -> str:
        """Construct question with temporal anchor context."""
        anchor_date = task.extra.get("anchor_date", "2024-01-01")
        header = self.ANSWER_CONTEXT_TEMPLATE.format(date=anchor_date)
        return header + task.question.strip()

    def extract_prediction(
        self,
        task: BenchmarkTask,
        raw_result: Dict[str, Any],
    ) -> BenchmarkPrediction:
        """Extract prediction for AML benchmark."""
        raw_text = raw_result.get("text", "")
        extracted_answer = AnswerNormalizer.extract_raw_answer(raw_text)

        return BenchmarkPrediction(
            task_id=task.task_id,
            model_answer=extracted_answer,
            reasoning_trace=raw_text,
            raw_text=raw_result.get("raw_text", raw_text),
            duration_seconds=raw_result.get("duration_seconds", 0.0),
            tokens=raw_result.get("usage", {}),
            tools_called=raw_result.get("tools", {}),
            model=raw_result.get("model", ""),
            error=raw_result.get("error"),
            extra={"question_type": task.extra.get("question_type", "general")},
        )

    def evaluate_prediction(
        self,
        task: BenchmarkTask,
        prediction: BenchmarkPrediction,
    ) -> Optional[EvaluationResult]:
        """Local grading for AML memory questions."""
        if not task.ground_truth:
            return None

        is_correct, norm_model, norm_gold = AnswerNormalizer.compare_answers(
            prediction.model_answer,
            task.ground_truth,
            benchmark="aml",
        )

        return EvaluationResult(
            task_id=task.task_id,
            is_correct=is_correct,
            score=1.0 if is_correct else 0.0,
            gold_answer=task.ground_truth,
            model_answer=prediction.model_answer,
            normalized_gold=norm_gold,
            normalized_model=norm_model,
            details={"question_type": task.extra.get("question_type", "general")},
        )

    def format_submission_line(self, prediction: BenchmarkPrediction) -> Dict[str, Any]:
        """AML Leaderboard submission schema."""
        return {
            "task_id": prediction.task_id,
            "question_id": prediction.task_id,
            "prediction": prediction.model_answer,
            "full_response": prediction.reasoning_trace,
            "model": prediction.model,
            "question_type": prediction.extra.get("question_type", "general"),
            "tokens": prediction.tokens,
            "duration_seconds": prediction.duration_seconds,
        }
