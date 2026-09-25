"""Adapters module for official benchmark evaluation."""

from .base import BaseBenchmarkAdapter
from .gaia_adapter import GaiaBenchmarkAdapter
from .aml_adapter import AmlBenchmarkAdapter

ADAPTERS = {
    "gaia": GaiaBenchmarkAdapter,
    "aml": AmlBenchmarkAdapter,
}

def get_adapter(name: str) -> BaseBenchmarkAdapter:
    name_clean = name.strip().lower()
    if name_clean not in ADAPTERS:
        raise ValueError(f"Unknown benchmark: {name}. Supported: {list(ADAPTERS.keys())}")
    return ADAPTERS[name_clean]()

__all__ = [
    "BaseBenchmarkAdapter",
    "GaiaBenchmarkAdapter",
    "AmlBenchmarkAdapter",
    "get_adapter",
]
