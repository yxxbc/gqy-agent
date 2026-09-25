#!/usr/bin/env python3
"""Official Benchmark Submission Runner for gqy-agent.

Supports:
- GAIA (General AI Assistants Benchmark)
- AML (Agent Memory Leaderboard / LongMemEval)
- Sandboxed execution with isolated credentials and environment
- Automatic multimodal attachment extraction and workspace binding
- Real-time streaming evaluation and resume-on-failure support
- Compliant output generation for Hugging Face Leaderboard & GitHub PR submissions
"""

import argparse
import collections
import json
import os
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional

sys.path.insert(0, str(Path(__file__).resolve().parent))

from core.models import BenchmarkPrediction, BenchmarkReport, BenchmarkTask, EvaluationResult
from core.sandbox import GQYSandbox
from adapters import get_adapter
from tools.validate_submission import validate_submission_file
from tools.package_pr import package_submission


def run_benchmark(
    benchmark: str = "gaia",
    split: str = "validation",
    dataset_path: Optional[str] = None,
    output_dir: Optional[str] = None,
    model: Optional[str] = None,
    limit: Optional[int] = None,
    seed: int = 42,
    task_ids: Optional[List[str]] = None,
    level: Optional[int] = None,
    timeout: int = 300,
    resume: bool = False,
    dry_run: bool = False,
    no_tools: bool = False,
    tools: Optional[str] = None,
    auto_package: bool = False,
    team_name: str = "GQY AI Team",
) -> Dict[str, Any]:
    """Execute official benchmark evaluation pipeline."""
    start_wall_time = time.time()
    adapter = get_adapter(benchmark)

    # Resolve output directory
    timestamp_str = datetime.now().strftime("%Y%m%d_%H%M%S")
    out_dir = Path(output_dir or f"~/.cache/gqy-benchmarks/{benchmark}_{timestamp_str}").expanduser().resolve()
    out_dir.mkdir(parents=True, exist_ok=True)

    predictions_file = out_dir / "predictions.jsonl"
    eval_details_file = out_dir / "eval_details.jsonl"
    summary_file = out_dir / "summary.json"

    # Setup filters
    filters: Dict[str, Any] = {}
    if task_ids:
        filters["task_id"] = task_ids
    if level is not None:
        filters["level"] = level

    print(f"\n========================================================")
    print(f"🚀 Launching Official Benchmark Runner: {benchmark.upper()}")
    print(f"========================================================")
    print(f"Split:          {split}")
    print(f"Output Dir:     {out_dir}")
    print(f"Model:          {model or 'default from config'}")
    print(f"Dry Run:        {dry_run}")
    print(f"Resume Mode:    {resume}")
    print(f"Limit / Seed:   {limit or 'All'} / {seed}")
    print(f"========================================================\n")

    # Load tasks
    print("⏳ Loading benchmark dataset tasks...")
    tasks = adapter.load_tasks(
        data_source=dataset_path,
        split=split,
        limit=limit,
        seed=seed,
        filters=filters,
    )
    print(f"✅ Loaded {len(tasks)} benchmark tasks.")

    if not tasks:
        print("⚠️ No tasks matched criteria. Exiting.")
        return {"error": "No tasks loaded"}

    # Handle resume
    completed_task_ids = set()
    if resume and predictions_file.exists():
        with open(predictions_file, "r", encoding="utf-8") as f:
            for line in f:
                if line.strip():
                    try:
                        p_item = json.loads(line)
                        t_id = p_item.get("task_id") or p_item.get("question_id")
                        if t_id:
                            completed_task_ids.add(str(t_id))
                    except Exception:
                        pass
        print(f"🔄 Resuming: Found {len(completed_task_ids)} previously evaluated tasks. Skipping them.")

    # Initialize sandbox
    sandbox_base = out_dir / "sandbox"
    memory_needed = (benchmark.lower() == "aml")
    sandbox = GQYSandbox(base_dir=sandbox_base, memory_enabled=memory_needed)
    sandbox.setup()

    predictions: List[BenchmarkPrediction] = []
    eval_results: List[EvaluationResult] = []
    category_stats = collections.defaultdict(lambda: {"total": 0, "correct": 0})
    total_tokens = collections.Counter()
    latencies: List[float] = []

    # Open files for append
    pred_mode = "a" if resume else "w"
    with open(predictions_file, pred_mode, encoding="utf-8") as pred_out, \
         open(eval_details_file, pred_mode, encoding="utf-8") as eval_out:

        for idx, task in enumerate(tasks, start=1):
            if str(task.task_id) in completed_task_ids:
                continue

            print(f"\n[{idx}/{len(tasks)}] Task: {task.task_id} (Level: {task.level or 'N/A'})")
            print(f"   Question: {task.question[:120]}{'...' if len(task.question) > 120 else ''}")

            # Task workspace directory
            task_dir = sandbox.workspace / f"task_{task.task_id}"
            task_env_args = adapter.prepare_task_environment(task, sandbox, task_dir)

            prompt = adapter.build_prompt(task)
            system_prompt = adapter.build_system_prompt(task)

            # Run agent
            t0 = time.time()
            turn_result = sandbox.run_turn(
                prompt=prompt,
                cwd=task_env_args.get("cwd", task_dir),
                model=model,
                system_prompt=system_prompt,
                append_system_prompt=task_env_args.get("append_system_prompt"),
                tools=tools,
                no_tools=no_tools,
                images=task_env_args.get("images"),
                timeout=timeout,
                dry_run=dry_run,
            )
            elapsed = round(time.time() - t0, 2)
            latencies.append(elapsed)

            # Extract prediction
            prediction = adapter.extract_prediction(task, turn_result)
            predictions.append(prediction)

            # Update token stats
            for k, v in prediction.tokens.items():
                if isinstance(v, (int, float)):
                    total_tokens[k] += v

            # Format submission line and flush
            sub_line = adapter.format_submission_line(prediction)
            pred_out.write(json.dumps(sub_line, ensure_ascii=False) + "\n")
            pred_out.flush()

            # Local evaluation if ground truth exists
            eval_res = adapter.evaluate_prediction(task, prediction)
            status_text = ""
            if eval_res:
                eval_results.append(eval_res)
                eval_out.write(json.dumps(eval_res.to_dict(), ensure_ascii=False) + "\n")
                eval_out.flush()

                cat = str(task.level if task.level is not None else task.extra.get("question_type", "general"))
                category_stats[cat]["total"] += 1
                if eval_res.is_correct:
                    category_stats[cat]["correct"] += 1

                verdict = "✅ CORRECT" if eval_res.is_correct else "❌ WRONG"
                status_text = f" | {verdict} (Gold: '{task.ground_truth}')"

            print(f"   Answer: '{prediction.model_answer}' ({elapsed}s){status_text}")
            if prediction.tools_called:
                print(f"   Tools:  {dict(prediction.tools_called)}")
            if prediction.error:
                print(f"   ⚠️ Error: {prediction.error}")

    # Summary metrics calculation
    total_evaluated = len(predictions)
    completed_count = sum(1 for p in predictions if not p.error and p.model_answer)
    failed_count = sum(1 for p in predictions if p.error)

    accuracy = None
    if eval_results:
        correct_count = sum(1 for r in eval_results if r.is_correct)
        accuracy = round(correct_count / len(eval_results), 4)

    avg_latency = round(sum(latencies) / max(len(latencies), 1), 2)

    cat_breakdown = {}
    for cat, stats in category_stats.items():
        acc = round(stats["correct"] / stats["total"], 4) if stats["total"] > 0 else 0.0
        cat_breakdown[cat] = {
            "total": stats["total"],
            "correct": stats["correct"],
            "accuracy": acc,
        }

    report = BenchmarkReport(
        benchmark_name=benchmark.upper(),
        split=split,
        model_name=model or (predictions[0].model if predictions else "default"),
        total_tasks=total_evaluated,
        completed_tasks=completed_count,
        failed_tasks=failed_count,
        accuracy=accuracy,
        latency_avg=avg_latency,
        total_tokens=dict(total_tokens),
        category_scores=cat_breakdown,
        timestamp=datetime.now(timezone.utc).isoformat(),
    )

    summary_file.write_text(report.to_json(), encoding="utf-8")

    # Validate output submission
    is_valid, val_report = validate_submission_file(predictions_file, benchmark=benchmark)

    print("\n========================================================")
    print("📊 BENCHMARK RUN COMPLETED")
    print("========================================================")
    print(f"Benchmark:        {benchmark.upper()} ({split})")
    print(f"Evaluated Tasks:  {total_evaluated}")
    print(f"Successful:       {completed_count}")
    print(f"Failed:           {failed_count}")
    if accuracy is not None:
        print(f"Overall Accuracy: {accuracy * 100:.2f}%")
        for cat, c_data in cat_breakdown.items():
            print(f"  • Category/Level {cat}: {c_data['correct']}/{c_data['total']} ({c_data['accuracy']*100:.1f}%)")
    print(f"Avg Latency:      {avg_latency}s")
    print(f"Submission Valid: {'✅ YES' if is_valid else '❌ NO'}")
    print(f"Results File:     {predictions_file}")
    print(f"Summary File:     {summary_file}")
    print("========================================================\n")

    # Packaging if requested
    if auto_package:
        pack_dir = out_dir / "submission_package"
        package_submission(
            run_dir=out_dir,
            output_dir=pack_dir,
            benchmark=benchmark,
            model_name=report.model_name,
            team_name=team_name,
        )

    return report.to_dict()


def main():
    parser = argparse.ArgumentParser(description="Official Benchmark Submission Runner for gqy-agent.")
    parser.add_argument("--benchmark", default="gaia", choices=["gaia", "aml"], help="Benchmark name (gaia, aml)")
    parser.add_argument("--split", default="validation", help="Benchmark split (validation, test, level1, level2, level3)")
    parser.add_argument("--dataset", help="Path to local dataset file or directory")
    parser.add_argument("--output-dir", help="Directory where evaluation outputs will be stored")
    parser.add_argument("--model", help="Model specification (e.g., provider/model)")
    parser.add_argument("--limit", type=int, help="Limit number of tasks to evaluate")
    parser.add_argument("--seed", type=int, default=42, help="Random seed for sampling")
    parser.add_argument("--task-id", nargs="+", help="Specific task ID(s) to evaluate")
    parser.add_argument("--level", type=int, help="Filter by difficulty level (GAIA: 1, 2, 3)")
    parser.add_argument("--timeout", type=int, default=300, help="Per-task timeout in seconds")
    parser.add_argument("--resume", action="store_true", help="Resume interrupted run skipping completed tasks")
    parser.add_argument("--dry-run", action="store_true", help="Simulate execution without LLM calls")
    parser.add_argument("--no-tools", action="store_true", help="Disable tool execution")
    parser.add_argument("--tools", help="Comma-separated tool whitelist")
    parser.add_argument("--package-submission", action="store_true", help="Automatically package submission bundle")
    parser.add_argument("--team-name", default="GQY AI Team", help="Team name for submission card")

    args = parser.parse_args()

    run_benchmark(
        benchmark=args.benchmark,
        split=args.split,
        dataset_path=args.dataset,
        output_dir=args.output_dir,
        model=args.model,
        limit=args.limit,
        seed=args.seed,
        task_ids=args.task_id,
        level=args.level,
        timeout=args.timeout,
        resume=args.resume,
        dry_run=args.dry_run,
        no_tools=args.no_tools,
        tools=args.tools,
        auto_package=args.package_submission,
        team_name=args.team_name,
    )


if __name__ == "__main__":
    main()
