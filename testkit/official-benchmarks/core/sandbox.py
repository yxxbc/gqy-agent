"""Isolated execution sandbox and runner for gqy-agent."""

import collections
import json
import os
import shutil
import sqlite3
import subprocess
import time
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple


class GQYSandbox:
    """Manages an isolated execution environment for running gqy-agent on benchmark tasks."""

    def __init__(
        self,
        base_dir: Path,
        bin_path: Optional[Path] = None,
        src_home: Optional[Path] = None,
        memory_enabled: bool = False,
    ):
        self.base_dir = Path(base_dir).resolve()
        self.bin_path = Path(bin_path or os.environ.get("BIN", "~/.cargo/bin/gqy")).expanduser().resolve()
        self.src_home = Path(src_home or os.environ.get("SRC_HOME", "~/.gqy")).expanduser().resolve()
        self.memory_enabled = memory_enabled

        self.home = self.base_dir / "home"
        self.runtime = self.base_dir / "run"
        self.workspace = self.base_dir / "workspace"
        
        self.env = dict(os.environ)
        self.env["GQY_HOME"] = str(self.home)
        self.env["XDG_RUNTIME_DIR"] = str(self.runtime)
        self.env.pop("GQY_WEB_DIR", None)

    def setup(self):
        """Prepare clean directory structure and copy essential provider credentials."""
        if self.base_dir.exists():
            shutil.rmtree(self.base_dir)

        self.home.mkdir(parents=True, exist_ok=True)
        self.runtime.mkdir(parents=True, exist_ok=True)
        self.workspace.mkdir(parents=True, exist_ok=True)

        # Copy provider / model credentials without chat history or personal data
        dst_config = self.home / "config" / "config.jsonc"
        dst_config.parent.mkdir(parents=True, exist_ok=True)
        copied = False
        try:
            src_config = self.src_home / "config" / "config.jsonc"
            if src_config.exists():
                try:
                    # Read json or jsonc (handle potential comments simply or direct copy)
                    content = src_config.read_text(encoding="utf-8")
                    # Clean extraneous keys if valid json
                    try:
                        cfg = json.loads(content)
                        for key in ("platforms", "voice", "mcp", "notifications", "system_prompt_file", "skills"):
                            cfg.pop(key, None)
                        cfg.setdefault("tools", {})["enabled"] = True
                        mem = cfg.setdefault("memory", {})
                        mem["enabled"] = self.memory_enabled
                        dst_config.write_text(json.dumps(cfg, ensure_ascii=False, indent=2), encoding="utf-8")
                        copied = True
                    except Exception:
                        dst_config.write_text(content, encoding="utf-8")
                        copied = True
                except Exception:
                    try:
                        shutil.copy2(src_config, dst_config)
                        copied = True
                    except Exception:
                        pass
        except Exception:
            pass

        if not copied and not dst_config.exists():
            default_cfg = {
                "tools": {"enabled": True},
                "memory": {"enabled": self.memory_enabled}
            }
            dst_config.write_text(json.dumps(default_cfg, indent=2), encoding="utf-8")

        # Copy models cache if present to avoid cold API probes
        try:
            src_cache = self.src_home / "cache" / "models_cache.json"
            dst_cache = self.home / "cache" / "models_cache.json"
            if src_cache.exists():
                dst_cache.parent.mkdir(parents=True, exist_ok=True)
                try:
                    shutil.copy2(src_cache, dst_cache)
                except Exception:
                    pass
        except Exception:
            pass

    def cleanup(self):
        """Tear down temporary directories."""
        if self.base_dir.exists():
            try:
                shutil.rmtree(self.base_dir)
            except Exception:
                pass

    def get_memory_db_path(self) -> Path:
        """Locate or prepare SQLite memory database."""
        db_path = self.home / "personas" / "default" / "memory" / "memory.db"
        db_path.parent.mkdir(parents=True, exist_ok=True)
        return db_path

    def init_memory_schema(self, db_path: Path):
        """Initialize standard gqy memory schema if not yet present."""
        con = sqlite3.connect(db_path)
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
        con.commit()
        con.close()

    def run_turn(
        self,
        prompt: str,
        cwd: Optional[Path] = None,
        model: Optional[str] = None,
        system_prompt: Optional[str] = None,
        append_system_prompt: Optional[str] = None,
        tools: Optional[str] = None,
        no_tools: bool = False,
        images: Optional[List[Path]] = None,
        timeout: int = 300,
        dry_run: bool = False,
    ) -> Dict[str, Any]:
        """Execute a single agent evaluation turn with full event streaming and metrics tracking."""
        if dry_run:
            return {
                "text": f"[DRY-RUN RESULT] Simulated answer for: {prompt[:80]}",
                "raw_text": f"[DRY-RUN RESULT] Simulated answer for: {prompt[:80]}",
                "usage": {"prompt_tokens": 100, "completion_tokens": 20, "total_tokens": 120},
                "tools": {},
                "model": model or "dry-run-mock",
                "duration_seconds": 0.05,
                "error": None,
            }

        work_dir = (cwd or self.workspace).resolve()
        cmd = [
            str(self.bin_path),
            "ask",
            "--mode", "dev",
            "--output-format", "stream-json",
            "--cwd", str(work_dir),
        ]

        if not self.memory_enabled:
            cmd.append("--no-memory")
        if model:
            cmd.extend(["--model", model])
        if system_prompt:
            cmd.extend(["--system-prompt", system_prompt])
        if append_system_prompt:
            cmd.extend(["--append-system-prompt", append_system_prompt])
        if no_tools:
            cmd.append("--no-tools")
        elif tools:
            cmd.extend(["--tools", tools])

        if images:
            for img in images:
                if Path(img).exists():
                    cmd.extend(["--image", str(img)])

        cmd.extend(["--timeout", str(timeout)])
        cmd.append(prompt)

        start_time = time.time()
        try:
            proc = subprocess.run(
                cmd,
                env=self.env,
                cwd=str(work_dir),
                capture_output=True,
                text=True,
                timeout=timeout + 30,  # Grace period beyond internal gqy timeout
            )
        except subprocess.TimeoutExpired:
            return {
                "text": "",
                "raw_text": "",
                "usage": {},
                "tools": {},
                "model": model or "",
                "duration_seconds": round(time.time() - start_time, 2),
                "error": f"Timeout expired after {timeout} seconds",
            }
        except Exception as e:
            return {
                "text": "",
                "raw_text": "",
                "usage": {},
                "tools": {},
                "model": model or "",
                "duration_seconds": round(time.time() - start_time, 2),
                "error": f"Execution failed: {str(e)}",
            }

        duration = round(time.time() - start_time, 2)
        usage = collections.Counter()
        tools_called = collections.Counter()
        final_text = ""
        resolved_model = model or ""
        error_msg = None

        # Parse stream-json events from stdout
        for line in proc.stdout.splitlines():
            line = line.strip()
            if not line:
                continue
            try:
                event = json.loads(line)
            except json.JSONDecodeError:
                continue

            event_type = event.get("type")
            if event_type == "usage" and isinstance(event.get("usage"), dict):
                for k, v in event["usage"].items():
                    if isinstance(v, (int, float)):
                        usage[k] += v
                if "model" in event:
                    resolved_model = event["model"]
            elif event_type == "tool" and event.get("phase") in ("start", "started", "call"):
                tools_called[event.get("name", "unknown")] += 1
            elif event_type == "done":
                final_text = event.get("text", "")
                if "usage" in event and isinstance(event["usage"], dict):
                    for k, v in event["usage"].items():
                        if isinstance(v, (int, float)):
                            usage[k] = max(usage.get(k, 0), v)
                if "model" in event:
                    resolved_model = event["model"]
            elif event_type == "error":
                error_msg = event.get("message") or str(event)

        if not final_text and error_msg is None:
            if proc.returncode != 0:
                error_msg = (proc.stderr or proc.stdout).strip()[-500:] or f"Exit code {proc.returncode}"
            else:
                final_text = proc.stdout.strip()

        return {
            "text": final_text,
            "raw_text": proc.stdout,
            "usage": dict(usage),
            "tools": dict(tools_called),
            "model": resolved_model,
            "duration_seconds": duration,
            "error": error_msg,
        }
