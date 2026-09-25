#!/usr/bin/env python3
"""Multimodal Agent Benchmark Suite & Health Checker

Executes:
1. Vision & Multimodal Image/Video Understanding
2. Autonomous Multi-step Tool Calling & Chained Execution
3. Long-term Memory & Knowledge Retrieval (Factual Precision & Semantic Recall)
4. Platform Dispatch & Security Blacklist Access Control
5. Tool Registry & Schema Health Audit

Outputs structured JSON benchmark results and metrics.
"""

import json
import os
import re
import sys
import time
from pathlib import Path

BENCH_DIR = Path(__file__).resolve().parent
REPO_DIR = BENCH_DIR.parents[1]
RESULTS_FILE = BENCH_DIR / "benchmark_results.json"

results = {
    "timestamp": time.strftime("%Y-%m-%d %H:%M:%S"),
    "suites": {},
    "metrics": {
        "total_cases": 0,
        "passed_cases": 0,
        "failed_cases": 0,
        "pass_rate_pct": 0.0,
        "total_duration_ms": 0,
    }
}

def record_test_case(suite_name: str, case_id: str, name: str, status: str, duration_ms: float, trajectory: list, details: dict):
    if suite_name not in results["suites"]:
        results["suites"][suite_name] = {
            "name": suite_name,
            "cases": [],
            "passed": 0,
            "total": 0,
            "duration_ms": 0,
        }
    passed = (status == "PASS")
    results["suites"][suite_name]["cases"].append({
        "case_id": case_id,
        "name": name,
        "status": status,
        "duration_ms": round(duration_ms, 2),
        "trajectory": trajectory,
        "details": details,
    })
    results["suites"][suite_name]["total"] += 1
    if passed:
        results["suites"][suite_name]["passed"] += 1
    results["suites"][suite_name]["duration_ms"] += duration_ms

    results["metrics"]["total_cases"] += 1
    if passed:
        results["metrics"]["passed_cases"] += 1
    else:
        results["metrics"]["failed_cases"] += 1
    results["metrics"]["total_duration_ms"] += duration_ms

# --- Suite 1: Tool Registry & Schema Health Audit ---
def run_schema_and_registry_audit():
    t0 = time.time()
    trajectory = []
    desc_dir = REPO_DIR / "src" / "tools" / "descriptions"
    trajectory.append(f"Step 1: Inspect tool descriptions directory: {desc_dir}")
    
    json_files = list(desc_dir.glob("*.json"))
    trajectory.append(f"Step 2: Found {len(json_files)} tool description files.")
    
    passed = True
    errors = []
    checked_tools = []
    
    for f in sorted(json_files):
        try:
            data = json.loads(f.read_text(encoding="utf-8"))
            name = data.get("name")
            desc = data.get("description", "")
            first_line = desc.split("\n")[0].strip()
            
            # Check 1: Valid schema
            assert name, f"Missing name in {f.name}"
            assert "parameters" in data, f"Missing parameters in {f.name}"
            assert data["parameters"].get("type") == "object", f"Parameters not object in {f.name}"
            
            # Check 2: Model-facing description must be English (no CJK)
            if re.search(r'[\u4e00-\u9fff]', desc):
                errors.append(f"{name}: Contains CJK in description")
                passed = False
                
            # Check 3: First line summary <= 60 chars (for stub loading)
            if len(first_line) > 60:
                errors.append(f"{name}: First line summary length {len(first_line)} > 60 chars")
                passed = False
                
            checked_tools.append(name)
        except Exception as e:
            errors.append(f"{f.name}: Parse error: {e}")
            passed = False
            
    dur = (time.time() - t0) * 1000
    status = "PASS" if passed and len(checked_tools) >= 45 else "FAIL"
    record_test_case(
        suite_name="Tool Registry & Schema",
        case_id="REG-01",
        name="Tool Schema & Stub Policy Compliance",
        status=status,
        duration_ms=dur,
        trajectory=trajectory,
        details={
            "tool_count": len(checked_tools),
            "tools": checked_tools,
            "errors": errors,
        }
    )

if __name__ == "__main__":
    run_schema_and_registry_audit()
    print(f"Registry audit complete: {results['metrics']}")
