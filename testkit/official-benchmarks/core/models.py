"""Standard data models and schemas for official benchmark evaluations."""

from dataclasses import dataclass, field, asdict
from typing import Any, Dict, List, Optional
import json


@dataclass
class BenchmarkTask:
    """Represents a unified benchmark evaluation question/task."""
    task_id: str
    question: str
    benchmark: str  # "gaia", "aml", etc.
    split: str      # "validation", "test", etc.
    level: Optional[int] = None
    file_name: Optional[str] = None
    file_path: Optional[str] = None
    file_url: Optional[str] = None
    ground_truth: Optional[str] = None
    metadata: Dict[str, Any] = field(default_factory=dict)
    extra: Dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> Dict[str, Any]:
        return asdict(self)

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "BenchmarkTask":
        return cls(
            task_id=str(data.get("task_id", "")),
            question=str(data.get("question", "")),
            benchmark=str(data.get("benchmark", "")),
            split=str(data.get("split", "validation")),
            level=data.get("level"),
            file_name=data.get("file_name"),
            file_path=data.get("file_path"),
            file_url=data.get("file_url"),
            ground_truth=data.get("ground_truth"),
            metadata=data.get("metadata", {}),
            extra=data.get("extra", {}),
        )


@dataclass
class BenchmarkPrediction:
    """Represents model prediction and runtime metrics for a task."""
    task_id: str
    model_answer: str
    reasoning_trace: Optional[str] = None
    raw_text: str = ""
    duration_seconds: float = 0.0
    tokens: Dict[str, int] = field(default_factory=dict)
    tools_called: Dict[str, int] = field(default_factory=dict)
    model: str = ""
    error: Optional[str] = None
    extra: Dict[str, Any] = field(default_factory=dict)

    def to_submission_dict(self, benchmark: str) -> Dict[str, Any]:
        """Convert to official submission format for leaderboard or PR."""
        if benchmark.lower() == "gaia":
            res = {
                "task_id": self.task_id,
                "model_answer": self.model_answer,
            }
            if self.reasoning_trace:
                res["reasoning_trace"] = self.reasoning_trace
            return res
        elif benchmark.lower() == "aml":
            return {
                "task_id": self.task_id,
                "question_id": self.task_id,
                "prediction": self.model_answer,
                "model": self.model,
                "tools_used": list(self.tools_called.keys()),
                "duration_seconds": self.duration_seconds,
                "tokens": self.tokens,
            }
        else:
            return {
                "task_id": self.task_id,
                "model_answer": self.model_answer,
                "raw_text": self.raw_text,
                "model": self.model,
            }

    def to_full_dict(self) -> Dict[str, Any]:
        return asdict(self)


@dataclass
class EvaluationResult:
    """Local grading evaluation result comparing prediction with ground truth."""
    task_id: str
    is_correct: Optional[bool] = None
    score: float = 0.0
    gold_answer: Optional[str] = None
    model_answer: str = ""
    normalized_gold: Optional[str] = None
    normalized_model: str = ""
    details: Dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> Dict[str, Any]:
        return asdict(self)


@dataclass
class BenchmarkReport:
    """Summary report for an evaluation run."""
    benchmark_name: str
    split: str
    model_name: str
    total_tasks: int
    completed_tasks: int
    failed_tasks: int
    accuracy: Optional[float] = None
    latency_avg: float = 0.0
    total_tokens: Dict[str, int] = field(default_factory=dict)
    category_scores: Dict[str, Any] = field(default_factory=dict)
    timestamp: str = ""

    def to_dict(self) -> Dict[str, Any]:
        return asdict(self)

    def to_json(self, indent: int = 2) -> str:
        return json.dumps(self.to_dict(), indent=indent, ensure_ascii=False)
