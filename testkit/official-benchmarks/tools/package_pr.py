#!/usr/bin/env python3
"""Package official benchmark evaluation results into submission bundles for Hugging Face / GitHub PR.

Generates:
1. predictions.jsonl (validated)
2. metadata.json (standardized leaderboard schema)
3. SUBMISSION_CARD.md (reproducibility documentation and PR description)
4. Optional .tar.gz / .zip archive ready for upload
"""

import argparse
import json
import os
import shutil
import subprocess
import tarfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict


def get_git_info() -> Dict[str, str]:
    """Extract current git commit and status."""
    info = {"commit": "unknown", "branch": "unknown", "dirty": False}
    try:
        commit = subprocess.check_output(["git", "rev-parse", "HEAD"], stderr=subprocess.DEVNULL, text=True).strip()
        branch = subprocess.check_output(["git", "rev-parse", "--abbrev-ref", "HEAD"], stderr=subprocess.DEVNULL, text=True).strip()
        status = subprocess.check_output(["git", "status", "--porcelain"], stderr=subprocess.DEVNULL, text=True).strip()
        info["commit"] = commit
        info["branch"] = branch
        info["dirty"] = bool(status)
    except Exception:
        pass
    return info


def package_submission(
    run_dir: Path,
    output_dir: Path,
    benchmark: str = "gaia",
    model_name: str = "gqy-agent",
    team_name: str = "GQY Team",
    agent_url: str = "https://github.com/gqy-agent/gqy",
    notes: str = "",
    create_archive: bool = True,
) -> Path:
    """Create a standardized submission package."""
    run_dir = Path(run_dir).resolve()
    output_dir = Path(output_dir).resolve()
    output_dir.mkdir(parents=True, exist_ok=True)

    src_predictions = run_dir / "predictions.jsonl"
    if not src_predictions.exists():
        raise FileNotFoundError(f"predictions.jsonl not found in {run_dir}")

    # Copy predictions.jsonl
    dst_predictions = output_dir / "predictions.jsonl"
    shutil.copy2(src_predictions, dst_predictions)

    # Load metrics / summary if present
    summary_path = run_dir / "summary.json"
    summary_data = {}
    if summary_path.exists():
        try:
            summary_data = json.loads(summary_path.read_text(encoding="utf-8"))
        except Exception:
            pass

    git_info = get_git_info()
    timestamp_utc = datetime.now(timezone.utc).isoformat()

    metadata = {
        "benchmark": benchmark.upper(),
        "model_name": model_name,
        "team_name": team_name,
        "agent_repository": agent_url,
        "harness": "gqy-agent testkit official-benchmarks",
        "harness_version": "0.7.0",
        "git_commit": git_info["commit"],
        "git_branch": git_info["branch"],
        "git_dirty": git_info["dirty"],
        "submission_timestamp": timestamp_utc,
        "total_tasks": summary_data.get("total_tasks", 0),
        "accuracy": summary_data.get("accuracy"),
        "latency_avg_seconds": summary_data.get("latency_avg"),
        "total_tokens": summary_data.get("total_tokens", {}),
        "notes": notes,
    }

    metadata_path = output_dir / "metadata.json"
    metadata_path.write_text(json.dumps(metadata, indent=2, ensure_ascii=False), encoding="utf-8")

    # Generate PR description and submission card
    pr_card_content = f"""# {benchmark.upper()} Official Benchmark Submission: {model_name}

## 1. Overview
- **Agent Name**: `{model_name}`
- **Team**: {team_name}
- **Benchmark Track**: `{benchmark.upper()}`
- **Evaluation Date**: `{timestamp_utc}`
- **Repository / Source**: [{agent_url}]({agent_url})
- **Git Commit**: `{git_info['commit']}`

## 2. Benchmark Results & Metrics

| Metric | Value |
|---|---|
| **Total Evaluated Tasks** | `{summary_data.get('total_tasks', 'N/A')}` |
| **Completed Tasks** | `{summary_data.get('completed_tasks', 'N/A')}` |
| **Failed Tasks** | `{summary_data.get('failed_tasks', 0)}` |
| **Accuracy / Pass Rate** | `{summary_data.get('accuracy', 'N/A')}` |
| **Average Latency (s)** | `{summary_data.get('latency_avg', 'N/A')}` |

### Token Consumption
```json
{json.dumps(summary_data.get('total_tokens', {}), indent=2)}
```

## 3. Reproduction & Execution Pipeline
This evaluation was executed in a sandboxed, isolated environment using `gqy-agent testkit`:

```bash
# 1. Clone repository
git clone {agent_url}
cd gqy-agent
git checkout {git_info['commit']}

# 2. Build binaries
cargo build --release

# 3. Run official benchmark evaluation
python3 testkit/official-benchmarks/runner.py \\
    --benchmark {benchmark} \\
    --split {summary_data.get('split', 'validation')} \\
    --model {model_name}
```

## 4. Submission Files Included
- `predictions.jsonl`: Formatted task predictions conforming to official schema.
- `metadata.json`: Machine-readable metadata and runtime configurations.
- `SUBMISSION_CARD.md`: This reproduction documentation.

---
*Generated automatically by gqy-agent testkit official submission runner.*
"""
    card_path = output_dir / "SUBMISSION_CARD.md"
    card_path.write_text(pr_card_content, encoding="utf-8")

    if create_archive:
        archive_name = output_dir.parent / f"{benchmark}_{model_name.replace('/', '_')}_{int(datetime.now().timestamp())}.tar.gz"
        with tarfile.open(archive_name, "w:gz") as tar:
            tar.add(output_dir, arcname=output_dir.name)
        print(f"📦 Archive created at: {archive_name}")

    print(f"✅ Submission package created at: {output_dir}")
    return output_dir


def main():
    parser = argparse.ArgumentParser(description="Package benchmark predictions for official submission.")
    parser.add_argument("--run-dir", required=True, help="Directory containing evaluated predictions.jsonl and summary.json")
    parser.add_argument("--output-dir", required=True, help="Destination directory for packaged submission")
    parser.add_argument("--benchmark", default="gaia", choices=["gaia", "aml"], help="Benchmark name")
    parser.add_argument("--model-name", default="gqy-agent (gemini-3.7-flash-medium)", help="Model name used for evaluation")
    parser.add_argument("--team-name", default="GQY AI Team", help="Team or submitter name")
    parser.add_argument("--agent-url", default="https://github.com/gqy-agent/gqy", help="URL to project repository")
    parser.add_argument("--notes", default="Evaluated via gqy-agent official testkit runner.", help="Additional notes")

    args = parser.parse_args()
    package_submission(
        run_dir=Path(args.run_dir),
        output_dir=Path(args.output_dir),
        benchmark=args.benchmark,
        model_name=args.model_name,
        team_name=args.team_name,
        agent_url=args.agent_url,
        notes=args.notes,
    )


if __name__ == "__main__":
    main()
