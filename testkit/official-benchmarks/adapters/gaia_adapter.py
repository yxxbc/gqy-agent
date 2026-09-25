"""Official GAIA (General AI Assistants) Benchmark Adapter.

Supports:
- Hugging Face dataset format (gaia-benchmark/GAIA)
- Local parquet / jsonl / json directory formats
- Multimodal attachments (images, PDFs, spreadsheets, audio, zip archives)
- Workspace setup and `--cwd` binding
- Official answer normalization and scoring rules
- Hugging Face Leaderboard submission export
"""

import collections
import json
import os
import random
import re
import shutil
import urllib.request
import zipfile
from pathlib import Path
from typing import Any, Dict, List, Optional

from core.models import BenchmarkPrediction, BenchmarkTask, EvaluationResult
from core.normalizer import AnswerNormalizer
from .base import BaseBenchmarkAdapter


class GaiaBenchmarkAdapter(BaseBenchmarkAdapter):
    """Adapter for the official GAIA benchmark."""

    HF_REPO = "gaia-benchmark/GAIA"
    HF_BASE_URL = "https://huggingface.co/datasets/gaia-benchmark/GAIA/resolve/main"

    GAIA_SYSTEM_PROMPT = (
        "You are an expert autonomous assistant evaluated on the GAIA benchmark.\n"
        "Solve the user's task step-by-step using your available tools.\n"
        "Inspect the current working directory for any attached files or data.\n"
        "IMPORTANT RULES FOR YOUR OUTPUT:\n"
        "1. Complete all intermediate reasoning and tool actions first.\n"
        "2. At the very end of your final response, you MUST provide the exact concise final answer on a new line in this exact format:\n"
        "   FINAL ANSWER: <answer>\n"
        "3. Follow these formatting constraints for the final answer:\n"
        "   - If the answer is a number, output only the numeric digits (do not append units or currency unless explicitly requested by the question).\n"
        "   - If the answer is a list of items, separate them with commas (e.g., item1, item2, item3).\n"
        "   - Do NOT add any pleasantries, conversational filler, or explanations after 'FINAL ANSWER:'."
    )

    @property
    def name(self) -> str:
        return "gaia"

    def load_tasks(
        self,
        data_source: Optional[str] = None,
        split: str = "validation",
        limit: Optional[int] = None,
        seed: int = 42,
        filters: Optional[Dict[str, Any]] = None,
    ) -> List[BenchmarkTask]:
        """Load GAIA tasks from local file/directory or sample data.
        
        Args:
            data_source: Path to metadata.jsonl, .parquet, or directory containing them.
            split: 'validation' or 'test'.
            limit: Maximum number of tasks to load.
            seed: Random seed for sampling.
            filters: Optional dict, e.g. {"level": 1} or {"task_id": [...]}.
        """
        raw_items: List[Dict[str, Any]] = []

        if data_source and Path(data_source).exists():
            path = Path(data_source)
            if path.is_file():
                if path.suffix == ".jsonl":
                    with open(path, "r", encoding="utf-8") as f:
                        for line in f:
                            if line.strip():
                                raw_items.append(json.loads(line))
                elif path.suffix == ".json":
                    with open(path, "r", encoding="utf-8") as f:
                        content = json.load(f)
                        raw_items = content if isinstance(content, list) else [content]
                elif path.suffix == ".parquet":
                    import pandas as pd
                    df = pd.read_parquet(path)
                    raw_items = df.to_dict(orient="records")
            elif path.is_dir():
                # Search for metadata.jsonl or parquet in dir
                candidates = list(path.glob("**/*metadata*.jsonl")) or list(path.glob("**/*.jsonl")) or list(path.glob("**/*.parquet"))
                if candidates:
                    return self.load_tasks(data_source=str(candidates[0]), split=split, limit=limit, seed=seed, filters=filters)
        else:
            # Fallback to packaged sample tasks if no external source given
            sample_file = Path(__file__).resolve().parent.parent / "samples" / "gaia_sample.jsonl"
            if sample_file.exists():
                with open(sample_file, "r", encoding="utf-8") as f:
                    for line in f:
                        if line.strip():
                            raw_items.append(json.loads(line))

        tasks: List[BenchmarkTask] = []
        for idx, item in enumerate(raw_items):
            # Parse GAIA keys
            task_id = str(item.get("task_id") or item.get("Question_id") or f"gaia_task_{idx:03d}")
            question = item.get("Question") or item.get("question") or ""
            level = item.get("Level") or item.get("level")
            if level is not None:
                try:
                    level = int(level)
                except ValueError:
                    level = None

            file_name = item.get("file_name") or item.get("file") or None
            if file_name and str(file_name).strip() in ("", "None", "nan"):
                file_name = None

            ground_truth = item.get("Final answer") or item.get("final_answer") or item.get("ground_truth")
            if ground_truth is not None:
                ground_truth = str(ground_truth).strip()

            annotator_metadata = item.get("Annotator Metadata") or {}

            # Apply filters
            if filters:
                if "level" in filters and level != filters["level"]:
                    continue
                if "task_id" in filters:
                    allowed = filters["task_id"]
                    if isinstance(allowed, list) and task_id not in allowed:
                        continue
                    elif isinstance(allowed, str) and task_id != allowed:
                        continue
                if "has_file" in filters:
                    has_f = bool(file_name)
                    if has_f != bool(filters["has_file"]):
                        continue

            task = BenchmarkTask(
                task_id=task_id,
                question=question,
                benchmark="gaia",
                split=split,
                level=level,
                file_name=file_name,
                ground_truth=ground_truth,
                metadata={
                    "annotator_metadata": annotator_metadata,
                    "raw_item": {k: v for k, v in item.items() if k not in ("Question", "Final answer")},
                }
            )
            tasks.append(task)

        # Sampling if limit specified
        if limit is not None and limit < len(tasks):
            rng = random.Random(seed)
            # Stratified sampling across levels
            by_level = collections.defaultdict(list)
            for t in tasks:
                by_level[t.level or 0].append(t)
            for lvl in by_level:
                rng.shuffle(by_level[lvl])
            
            sampled: List[BenchmarkTask] = []
            levels = sorted(by_level.keys())
            while len(sampled) < limit and any(by_level.values()):
                for lvl in levels:
                    if by_level[lvl] and len(sampled) < limit:
                        sampled.append(by_level[lvl].pop())
            tasks = sampled

        return tasks

    def prepare_task_environment(
        self,
        task: BenchmarkTask,
        sandbox: Any,
        task_dir: Path,
    ) -> Dict[str, Any]:
        """Setup task directory, copy attachments, unpack archives, identify images."""
        task_dir = Path(task_dir).resolve()
        task_dir.mkdir(parents=True, exist_ok=True)

        images: List[Path] = []
        extra_args: Dict[str, Any] = {
            "cwd": task_dir,
            "images": images,
            "append_system_prompt": self.GAIA_SYSTEM_PROMPT,
        }

        if not task.file_name:
            return extra_args

        # Locate file: check in task metadata, relative path, or sample dir
        candidate_paths = []
        if task.file_path and Path(task.file_path).exists():
            candidate_paths.append(Path(task.file_path))
        if "data_dir" in task.metadata:
            candidate_paths.append(Path(task.metadata["data_dir"]) / task.file_name)
        
        # Check standard sample attachments dir
        sample_attachments = Path(__file__).resolve().parent.parent / "samples" / "attachments" / task.file_name
        if sample_attachments.exists():
            candidate_paths.append(sample_attachments)

        source_file = None
        for p in candidate_paths:
            if p.exists():
                source_file = p
                break

        dest_file = task_dir / task.file_name

        if source_file and source_file.exists():
            shutil.copy2(source_file, dest_file)
        elif task.file_url:
            # Download from URL
            try:
                urllib.request.urlretrieve(task.file_url, dest_file)
            except Exception as e:
                task.metadata["file_download_error"] = str(e)

        # Post-process attachment
        if dest_file.exists():
            ext = dest_file.suffix.lower()
            if ext in (".png", ".jpg", ".jpeg", ".webp"):
                images.append(dest_file)
            elif ext == ".zip":
                try:
                    with zipfile.ZipFile(dest_file, "r") as zf:
                        zf.extractall(task_dir)
                except Exception:
                    pass

        return extra_args

    def build_prompt(self, task: BenchmarkTask) -> str:
        """Construct question prompt for GAIA."""
        prompt = task.question.strip()
        if task.file_name:
            prompt += (
                f"\n\n[Note: An attachment '{task.file_name}' has been placed in your current working directory. "
                f"You can inspect it using your tools.]"
            )
        return prompt

    def extract_prediction(
        self,
        task: BenchmarkTask,
        raw_result: Dict[str, Any],
    ) -> BenchmarkPrediction:
        """Extract prediction following official GAIA normalization rules."""
        raw_text = raw_result.get("text", "")
        raw_answer = AnswerNormalizer.extract_raw_answer(raw_text)
        normalized_answer = AnswerNormalizer.normalize_gaia(raw_answer)

        return BenchmarkPrediction(
            task_id=task.task_id,
            model_answer=normalized_answer,
            reasoning_trace=raw_text,
            raw_text=raw_result.get("raw_text", raw_text),
            duration_seconds=raw_result.get("duration_seconds", 0.0),
            tokens=raw_result.get("usage", {}),
            tools_called=raw_result.get("tools", {}),
            model=raw_result.get("model", ""),
            error=raw_result.get("error"),
            extra={"level": task.level, "has_file": bool(task.file_name)},
        )

    def evaluate_prediction(
        self,
        task: BenchmarkTask,
        prediction: BenchmarkPrediction,
    ) -> Optional[EvaluationResult]:
        """Perform official GAIA local grading if ground truth answer is present."""
        if not task.ground_truth:
            return None

        is_correct, norm_model, norm_gold = AnswerNormalizer.compare_answers(
            prediction.model_answer,
            task.ground_truth,
            benchmark="gaia",
        )

        return EvaluationResult(
            task_id=task.task_id,
            is_correct=is_correct,
            score=1.0 if is_correct else 0.0,
            gold_answer=task.ground_truth,
            model_answer=prediction.model_answer,
            normalized_gold=norm_gold,
            normalized_model=norm_model,
            details={
                "level": task.level,
                "has_file": bool(task.file_name),
            }
        )

    def format_submission_line(self, prediction: BenchmarkPrediction) -> Dict[str, Any]:
        """Official GAIA Leaderboard submission schema."""
        entry = {
            "task_id": prediction.task_id,
            "model_answer": prediction.model_answer,
        }
        if prediction.reasoning_trace:
            entry["reasoning_trace"] = prediction.reasoning_trace
        return entry
