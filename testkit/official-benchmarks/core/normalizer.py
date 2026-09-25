"""Official benchmark answer extraction and normalization utilities.

Implements strict GAIA and AML answer post-processing rules.
"""

import math
import re
from typing import Any, List, Optional, Tuple, Union


class AnswerNormalizer:
    """Normalizes model outputs and ground truth answers according to benchmark standards."""

    # Patterns for extracting explicit final answers
    FINAL_ANSWER_PATTERNS = [
        re.compile(r"(?:FINAL ANSWER|Final Answer|final answer)[:：]\s*(.+?)(?:\n\n|\Z)", re.DOTALL),
        re.compile(r"<answer>(.*?)</answer>", re.DOTALL | re.IGNORECASE),
        re.compile(r"\[answer\](.*?)\[/answer\]", re.DOTALL | re.IGNORECASE),
        re.compile(r"(?:The answer is|Therefore, the answer is)[:：]?\s*(.+?)(?:\.|\n|\Z)", re.IGNORECASE),
        re.compile(r"\*\*(?:Final Answer|Answer)\*\*[:：]?\s*(.+?)(?:\n\n|\Z)", re.DOTALL | re.IGNORECASE),
    ]

    @classmethod
    def extract_raw_answer(cls, text: str) -> str:
        """Extract the core answer string from assistant conversational output."""
        if not text:
            return ""
        
        # 1. Try explicit final answer tags/headers
        for pat in cls.FINAL_ANSWER_PATTERNS:
            match = pat.search(text)
            if match:
                candidate = match.group(1).strip()
                # If candidate has multiple lines, take the first coherent block
                candidate_lines = [l.strip() for l in candidate.splitlines() if l.strip()]
                if candidate_lines:
                    return candidate_lines[0]

        # 2. Look for markdown bold (ignoring label headers like '**Header:**')
        bold_matches = list(re.finditer(r"\*\*([^*]+)\*\*", text))
        valid_bolds = [
            m.group(1).strip() for m in bold_matches
            if not m.group(1).strip().endswith(":") and not m.group(1).strip().endswith("：")
        ]
        if valid_bolds:
            candidate = valid_bolds[-1]
            if len(candidate) < 120 and "\n" not in candidate:
                return candidate

        # 3. Fallback: inspect the last non-empty line
        lines = [line.strip() for line in text.strip().splitlines() if line.strip()]
        if lines:
            last_line = lines[-1]
            # Strip prefixes like "Answer:" or "Result:"
            last_line = re.sub(r"^(?:Answer|Result|Conclusion|Output)[:：]\s*", "", last_line, flags=re.IGNORECASE)
            return last_line

        return text.strip()

    @classmethod
    def normalize_number(cls, text: str) -> Optional[float]:
        """Attempt to parse text as a normalized float number, handling commas and units."""
        cleaned = text.strip()
        # Remove currency symbols and common units
        cleaned = re.sub(r"^[\$€£¥]\s*", "", cleaned)
        cleaned = re.sub(r"\s*[%％]$", "", cleaned)
        # Remove thousands separators e.g. 1,000,000 -> 1000000
        cleaned = re.sub(r"(?<=\d),(?=\d{3}(?:\D|$))", "", cleaned)
        try:
            val = float(cleaned)
            if not math.isnan(val) and not math.isinf(val):
                return val
        except ValueError:
            pass
        return None

    @classmethod
    def normalize_list(cls, text: str) -> List[str]:
        """Normalize comma-separated or list-formatted text into sorted string list."""
        cleaned = text.strip()
        # Strip brackets
        cleaned = re.sub(r"^\[(.*)\]$", r"\1", cleaned)
        cleaned = re.sub(r"^\((.*)\)$", r"\1", cleaned)
        # Split on comma or semicolon
        items = [cls.normalize_text(item) for item in re.split(r"[,;]\s*", cleaned) if item.strip()]
        return sorted(items)

    @classmethod
    def normalize_text(cls, text: str) -> str:
        """Strip surrounding punctuation, quotes, and excessive whitespace."""
        s = text.strip()
        # Remove quotes
        s = s.strip("\"'`“”‘’")
        # Remove trailing sentence punctuation
        s = re.sub(r"[.!?。！？]+$", "", s).strip()
        # Collapse whitespace
        s = " ".join(s.split())
        return s

    @classmethod
    def normalize_gaia(cls, text: str) -> str:
        """Official GAIA answer normalization pipeline.
        
        Rules:
        - If float/integer, format compactly (e.g. 12000 or 12.5).
        - If comma-separated list, strip extra spaces and sort if appropriate.
        - Strip common conversational fillers and punctuation.
        """
        extracted = cls.extract_raw_answer(text)
        cleaned = cls.normalize_text(extracted)

        # Check if it is a number
        num = cls.normalize_number(cleaned)
        if num is not None:
            # Check if it is an integer
            if abs(num - round(num)) < 1e-9:
                return str(int(round(num)))
            # Format float nicely up to 4 decimal places without trailing zeros
            formatted = f"{num:.4f}".rstrip("0").rstrip(".")
            return formatted

        # Check if it looks like a list
        if "," in cleaned or ";" in cleaned:
            items = cls.normalize_list(cleaned)
            if len(items) > 1:
                return ", ".join(items)

        return cleaned

    @classmethod
    def compare_answers(cls, model_answer: str, gold_answer: str, benchmark: str = "gaia") -> Tuple[bool, str, str]:
        """Compare model prediction with gold truth under benchmark rules.
        
        Returns:
            (is_correct, normalized_model, normalized_gold)
        """
        if benchmark.lower() == "gaia":
            norm_gold = cls.normalize_gaia(gold_answer)
            norm_model = cls.normalize_gaia(model_answer)

            # 1. Exact string match (case-insensitive)
            if norm_gold.lower() == norm_model.lower():
                return True, norm_model, norm_gold

            # 2. Number comparison with tolerance
            gold_num = cls.normalize_number(gold_answer)
            model_num = cls.normalize_number(model_answer)
            if gold_num is not None and model_num is not None:
                # Relative or absolute tolerance
                if abs(gold_num - model_num) <= 1e-3 or (abs(gold_num) > 1e-5 and abs(gold_num - model_num) / abs(gold_num) <= 1e-2):
                    return True, norm_model, norm_gold

            # 3. List comparison (unordered match)
            gold_items = cls.normalize_list(gold_answer)
            model_items = cls.normalize_list(model_answer)
            if len(gold_items) > 1 and len(gold_items) == len(model_items):
                if [x.lower() for x in sorted(gold_items)] == [y.lower() for y in sorted(model_items)]:
                    return True, norm_model, norm_gold

            return False, norm_model, norm_gold

        elif benchmark.lower() == "aml":
            norm_gold = cls.normalize_text(gold_answer).lower()
            norm_model = cls.normalize_text(model_answer).lower()

            # Abstention detection
            abstention_patterns = [
                r"don't know", r"do not know", r"cannot", r"not mentioned", r"never mentioned",
                r"unknown", r"no memory", r"no record", r"not recorded", r"unrecorded",
                r"(?:do not|don't|not|never|have no)\s+(?:have\s+|find\s+)?(?:any\s+|a\s+)?(?:record|memory|recollection|mention)\b",
                r"(?:was|were|is)\s+never\s+mentioned\b",
                r"no\s+recollection\b",
            ]
            gold_abstains = any(re.search(p, norm_gold, re.I) for p in abstention_patterns)
            model_abstains = any(re.search(p, norm_model, re.I) for p in abstention_patterns)

            if gold_abstains:
                return model_abstains, norm_model, norm_gold

            # In general, if gold is contained in model or exact match
            if norm_gold in norm_model or norm_model in norm_gold:
                return True, norm_model, norm_gold

            return False, norm_model, norm_gold

        else:
            norm_gold = cls.normalize_text(gold_answer).lower()
            norm_model = cls.normalize_text(model_answer).lower()
            return norm_gold == norm_model, norm_model, norm_gold
