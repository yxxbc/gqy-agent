"""Base adapter class for official benchmarks."""

from abc import ABC, abstractmethod
from pathlib import Path
from typing import Any, Dict, List, Optional

from core.models import BenchmarkTask, BenchmarkPrediction, EvaluationResult


class BaseBenchmarkAdapter(ABC):
    """Abstract base class that every official benchmark adapter must implement."""

    @property
    @abstractmethod
    def name(self) -> str:
        """Name of the benchmark (e.g., 'gaia', 'aml')."""
        pass

    @abstractmethod
    def load_tasks(
        self,
        data_source: Optional[str] = None,
        split: str = "validation",
        limit: Optional[int] = None,
        seed: int = 42,
        filters: Optional[Dict[str, Any]] = None,
    ) -> List[BenchmarkTask]:
        """Load tasks from dataset source (Hugging Face repo, local directory, or file)."""
        pass

    @abstractmethod
    def prepare_task_environment(
        self,
        task: BenchmarkTask,
        sandbox: Any,
        task_dir: Path,
    ) -> Dict[str, Any]:
        """Set up task-specific environment, copy/download attachments, seed memory.
        
        Returns a dict with execution arguments, e.g.:
            {"images": [...], "cwd": Path(...), "append_system_prompt": str, ...}
        """
        pass

    @abstractmethod
    def build_prompt(self, task: BenchmarkTask) -> str:
        """Construct prompt to present to the agent."""
        pass

    def build_system_prompt(self, task: BenchmarkTask) -> Optional[str]:
        """Construct optional system prompt for formatting or instruction adherence."""
        return None

    @abstractmethod
    def extract_prediction(
        self,
        task: BenchmarkTask,
        raw_result: Dict[str, Any],
    ) -> BenchmarkPrediction:
        """Extract prediction and metadata from execution output."""
        pass

    @abstractmethod
    def evaluate_prediction(
        self,
        task: BenchmarkTask,
        prediction: BenchmarkPrediction,
    ) -> Optional[EvaluationResult]:
        """Evaluate prediction against ground truth if available."""
        pass

    @abstractmethod
    def format_submission_line(self, prediction: BenchmarkPrediction) -> Dict[str, Any]:
        """Format single line for predictions.jsonl submission."""
        pass
