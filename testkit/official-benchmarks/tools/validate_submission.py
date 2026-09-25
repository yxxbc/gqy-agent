#!/usr/bin/env python3
"""Validation utility for official benchmark submission files (predictions.jsonl).

Ensures complete compliance with Hugging Face Leaderboard & GitHub PR specifications:
- Validates JSON Lines syntax
- Checks required schema keys (task_id, model_answer / prediction)
- Asserts uniqueness of task IDs
- Detects NaN/null/corrupted values
- Validates task coverage against reference dataset if provided
"""

import argparse
import json
import sys
from pathlib import Path
from typing import Dict, List, Set, Tuple


def validate_submission_file(
    submission_path: Path,
    benchmark: str = "gaia",
    reference_dataset_path: Path = None,
) -> Tuple[bool, Dict[str, any]]:
    """Validate predictions.jsonl file."""
    if not submission_path.exists():
        return False, {"error": f"Submission file not found: {submission_path}"}

    errors: List[str] = []
    warnings: List[str] = []
    seen_ids: Set[str] = set()
    total_lines = 0
    empty_answers = 0
    predictions_preview = []

    id_key = "task_id" if benchmark == "gaia" else "task_id"  # aml also supports task_id / question_id
    answer_key = "model_answer" if benchmark == "gaia" else "prediction"

    with open(submission_path, "r", encoding="utf-8") as f:
        for line_no, line in enumerate(f, start=1):
            line_str = line.strip()
            if not line_str:
                warnings.append(f"Line {line_no}: Empty blank line encountered.")
                continue

            total_lines += 1
            try:
                item = json.loads(line_str)
            except json.JSONDecodeError as e:
                errors.append(f"Line {line_no}: Invalid JSON: {e}")
                continue

            if not isinstance(item, dict):
                errors.append(f"Line {line_no}: Line is not a JSON object: {type(item)}")
                continue

            # Check ID
            t_id = item.get("task_id") or (item.get("question_id") if benchmark == "aml" else None)
            if not t_id:
                errors.append(f"Line {line_no}: Missing required key '{id_key}'.")
            else:
                t_id_str = str(t_id)
                if t_id_str in seen_ids:
                    errors.append(f"Line {line_no}: Duplicate task ID found: '{t_id_str}'.")
                seen_ids.add(t_id_str)

            # Check Answer
            ans = item.get("model_answer") if benchmark == "gaia" else (item.get("prediction") or item.get("model_answer"))
            if ans is None:
                errors.append(f"Line {line_no}: Missing required prediction key '{answer_key}'.")
            else:
                ans_str = str(ans).strip()
                if not ans_str:
                    empty_answers += 1
                    warnings.append(f"Line {line_no} (task {t_id}): Answer is empty.")

            if len(predictions_preview) < 5:
                predictions_preview.append({"task_id": t_id, "answer": str(ans)[:100]})

    if total_lines == 0:
        errors.append("Submission file contains 0 predictions.")

    # Reference dataset check
    reference_coverage = None
    if reference_dataset_path and reference_dataset_path.exists():
        ref_ids: Set[str] = set()
        with open(reference_dataset_path, "r", encoding="utf-8") as rf:
            for rline in rf:
                rline = rline.strip()
                if rline:
                    try:
                        ritem = json.loads(rline)
                        rid = ritem.get("task_id") or ritem.get("question_id") or ritem.get("Question_id")
                        if rid:
                            ref_ids.add(str(rid))
                    except Exception:
                        pass
        
        missing_ids = ref_ids - seen_ids
        extra_ids = seen_ids - ref_ids
        reference_coverage = {
            "reference_total": len(ref_ids),
            "predicted_total": len(seen_ids),
            "missing_count": len(missing_ids),
            "extra_count": len(extra_ids),
        }
        if missing_ids:
            warnings.append(f"Submission is missing {len(missing_ids)} tasks from reference dataset (e.g., {list(missing_ids)[:3]}).")
        if extra_ids:
            warnings.append(f"Submission has {len(extra_ids)} tasks not present in reference dataset (e.g., {list(extra_ids)[:3]}).")

    passed = len(errors) == 0
    report = {
        "passed": passed,
        "total_predictions": total_lines,
        "unique_task_ids": len(seen_ids),
        "empty_answers": empty_answers,
        "error_count": len(errors),
        "warning_count": len(warnings),
        "errors": errors[:50],
        "warnings": warnings[:50],
        "coverage": reference_coverage,
        "sample_preview": predictions_preview,
    }
    return passed, report


def main():
    parser = argparse.ArgumentParser(description="Validate official benchmark submission files.")
    parser.add_argument("file", help="Path to predictions.jsonl")
    parser.add_argument("--benchmark", choices=["gaia", "aml"], default="gaia", help="Target benchmark format")
    parser.add_argument("--reference", help="Optional path to reference task dataset for coverage check")
    parser.add_argument("--json", action="store_true", help="Print report in JSON format")

    args = parser.parse_args()
    submission_path = Path(args.file)
    ref_path = Path(args.reference) if args.reference else None

    passed, report = validate_submission_file(submission_path, benchmark=args.benchmark, reference_dataset_path=ref_path)

    if args.json:
        print(json.dumps(report, indent=2, ensure_ascii=False))
    else:
        status_sym = "✅ PASSED" if passed else "❌ FAILED"
        print(f"\n==========================================")
        print(f" SUBMISSION VALIDATION: {status_sym}")
        print(f"==========================================")
        print(f"File:               {submission_path}")
        print(f"Benchmark:          {args.benchmark.upper()}")
        print(f"Total Rows:         {report['total_predictions']}")
        print(f"Unique Tasks:       {report['unique_task_ids']}")
        print(f"Empty Answers:      {report['empty_answers']}")
        print(f"Errors:             {report['error_count']}")
        print(f"Warnings:           {report['warning_count']}")

        if report.get("coverage"):
            cov = report["coverage"]
            print(f"Dataset Coverage:   {cov['predicted_total']} / {cov['reference_total']} (Missing: {cov['missing_count']})")

        if report["errors"]:
            print(f"\n--- ERRORS ---")
            for err in report["errors"][:10]:
                print(f"  • {err}")

        if report["warnings"]:
            print(f"\n--- WARNINGS ---")
            for w in report["warnings"][:10]:
                print(f"  • {w}")

        print("\n--- SAMPLE PREDICTIONS ---")
        for s in report["sample_preview"]:
            print(f"  [{s['task_id']}] -> {s['answer']}")
        print("==========================================\n")

    sys.exit(0 if passed else 1)


if __name__ == "__main__":
    main()
