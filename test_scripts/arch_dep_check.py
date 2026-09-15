#!/usr/bin/env python3
"""模块依赖方向门禁：按层序表全对比较。

拆分要解决的不只是"文件太长"，还有"牵一发动全身"。反向依赖让底层模块反过来
引用上层，任何一处改动都会沿着它扩散。

旧版门禁手列了 8 条禁止边，其余约 190 条跨模块依赖不受管辖——盲区里新增一条
反向边不会被拦。现在改成一张**层序表**：每个顶层模块必须归入某一层，任何
"低层 → 高层"的引用都算违规，同层互引不管。新建顶层模块不归层直接失败，逼着
在建模块的那一刻就决定它在哪一层。

层（自底向上，与 docs/architecture.md 的三层 + 场所层对应）：

    0 基础      i18n paths shell prompts logging notify json_extract token_counter
                token_estimate memory_types platform_types slash_commands
    1 配置      config default_models models_cache
    2 基础设施  llm state embedding ipc question alarm skills pm transfer voice
                terminal persona_hint args
    3 能力      tools memory render ledger clipboard host_info default_kb
    4 回合引擎  agent runtime
    5 场所      platforms
    6 daemon    web
    7 入口      cli config_tui question_tui oobe daemon

白名单（同目录 `arch-dep-waivers.json`）记录**现存**违规的条数，每条写明为什么
还在。只减不增：新边直接失败，旧边变多失败；变少时提示用 `--tighten` 收紧。

用法：

    python3 test_scripts/arch_dep_check.py                 # 报告 + 白名单外的违规即失败
    python3 test_scripts/arch_dep_check.py --warn          # 只报告，永远退出 0
    python3 test_scripts/arch_dep_check.py --tighten       # 把白名单收紧到现状（只降不升）
    python3 test_scripts/arch_dep_check.py --write-waivers # 以现状重建白名单（换层序表时用）
"""
import argparse
import json
import re
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "src"
# 白名单跟脚本走,不认目录名——目录改名(scripts → test_scripts)后硬编码路径
# 会静默失效:找不到基线时每一条既有引用都被当成新增,门禁全红(08-26)。
WAIVER_FILE = Path(__file__).resolve().parent / "arch-dep-waivers.json"

LAYERS = [
    ("基础", {
        "i18n", "paths", "shell", "prompts", "logging", "notify", "json_extract",
        "token_counter", "token_estimate", "memory_types", "platform_types",
        "slash_commands",
    }),
    ("配置", {"config", "default_models", "models_cache"}),
    ("基础设施", {
        "llm", "state", "embedding", "ipc", "question", "alarm", "skills", "pm",
        "transfer", "voice", "terminal", "persona_hint", "args",
    }),
    ("能力", {"tools", "memory", "render", "ledger", "clipboard", "host_info", "default_kb"}),
    ("回合引擎", {"agent", "runtime"}),
    ("场所", {"platforms"}),
    ("daemon", {"web"}),
    ("入口", {"cli", "config_tui", "question_tui", "oobe", "daemon"}),
]
# crate 根与独立二进制不属于任何层。
UNLAYERED = {"lib", "main", "bin"}

USE_CRATE = re.compile(r"\buse\s+crate::([a-z_][a-z0-9_]*)")
CRATE_PATH = re.compile(r"\bcrate::([a-z_][a-z0-9_]*)::")


def layer_index():
    index = {}
    for rank, (_, modules) in enumerate(LAYERS):
        for module in modules:
            if module in index:
                raise SystemExit(f"层序表里 {module} 出现了两次")
            index[module] = rank
    return index


def module_of(path):
    """文件属于哪个顶层模块。`src/tools/web.rs` → `tools`，`src/cli.rs` → `cli`。"""
    rel = path.relative_to(SRC)
    return rel.parts[0][:-3] if len(rel.parts) == 1 else rel.parts[0]


def scan(rank):
    """返回 ({(来源, 目标): [(文件, 行号, 原文)]}, 未归层的模块集合)"""
    edges = defaultdict(list)
    unknown = set()
    for path in sorted(SRC.rglob("*.rs")):
        source = module_of(path)
        if source in UNLAYERED:
            continue
        if source not in rank:
            unknown.add(source)
            continue
        for number, line in enumerate(
            path.read_text(encoding="utf-8", errors="replace").split("\n"), 1
        ):
            if line.lstrip().startswith("//"):
                continue
            # 同一行可能同时命中两个正则（`use crate::web::X` 就是），
            # 按目标模块去重，否则计数虚高。
            targets = {
                match.group(1)
                for match in list(USE_CRATE.finditer(line))
                + list(CRATE_PATH.finditer(line))
            }
            for target in sorted(targets):
                if target == source or target not in rank:
                    continue
                if rank[target] > rank[source]:
                    edges[(source, target)].append(
                        (str(path.relative_to(ROOT)), number, line.strip()[:100])
                    )
    return edges, unknown


def key_of(source, target):
    return f"{source}->{target}"


def load_waivers():
    if not WAIVER_FILE.exists():
        return {}
    return json.loads(WAIVER_FILE.read_text(encoding="utf-8")).get("waivers", {})


def save_waivers(waivers, note):
    payload = {
        "_说明": "现存跨层引用（低层 → 高层）的白名单，层序表见 arch_dep_check.py。"
                 "只允许变小，变大即失败；新增的边不在表里，直接失败。",
        "_来源": note,
        "waivers": dict(sorted(waivers.items())),
    }
    WAIVER_FILE.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--warn", action="store_true", help="只报告，永远退出 0")
    parser.add_argument("--tighten", action="store_true", help="把白名单收紧到现状（只降不升）")
    parser.add_argument("--write-waivers", action="store_true", help="以现状重建白名单")
    args = parser.parse_args()

    rank = layer_index()
    names = {rank_: name for rank_, (name, _) in enumerate(LAYERS)}
    edges, unknown = scan(rank)
    waivers = load_waivers()

    print("跨层引用现状（低层 → 高层，按边聚合）：")
    if not edges:
        print("  无")
    for (source, target), hits in sorted(edges.items(), key=lambda kv: -len(kv[1])):
        allowed = waivers.get(key_of(source, target), {}).get("count")
        mark = ""
        if allowed is None:
            mark = "  ← 未列入白名单"
        elif len(hits) > allowed:
            mark = f"  ← 超出白名单（{allowed} → {len(hits)}）"
        elif len(hits) < allowed:
            mark = f"  （白名单 {allowed}，可收紧）"
        layers = f"{names[rank[source]]}→{names[rank[target]]}"
        print(f"  {source:<14} → {target:<12} {len(hits):>3} 处  [{layers}]{mark}")
        for name, number, text in hits[:3]:
            print(f"      {name}:{number}  {text}")
        if len(hits) > 3:
            print(f"      …另有 {len(hits) - 3} 处")

    if args.write_waivers:
        waivers = {
            key_of(source, target): {
                "count": len(hits),
                "reason": waivers.get(key_of(source, target), {}).get("reason")
                or "层序表启用前的既有依赖："
                + ", ".join(sorted({name for name, _, _ in hits})[:4]),
            }
            for (source, target), hits in edges.items()
        }
        save_waivers(waivers, "--write-waivers 以现状重建")
        print(f"\n白名单已写入 {WAIVER_FILE.relative_to(ROOT)}")
        return 0

    if args.tighten:
        tightened = {}
        for key, entry in waivers.items():
            source, target = key.split("->")
            actual = len(edges.get((source, target), []))
            if actual:
                tightened[key] = {**entry, "count": min(entry["count"], actual)}
        save_waivers(tightened, "--tighten 收紧")
        print(f"\n白名单已收紧：{len(waivers)} → {len(tightened)} 条边")
        return 0

    stale = [
        key for key in waivers
        if (tuple(key.split("->")) not in edges)
    ]
    if stale:
        print("\n白名单里已经消失的边（可 --tighten 删除）：" + "、".join(sorted(stale)))

    if args.warn:
        return 0

    problems = []
    for module in sorted(unknown):
        problems.append(f"顶层模块 {module} 没有归层：在 LAYERS 里给它定一层")
    for (source, target), hits in edges.items():
        allowed = waivers.get(key_of(source, target), {}).get("count")
        if allowed is None:
            problems.append(f"新增跨层引用 {source} → {target}（{len(hits)} 处）")
        elif len(hits) > allowed:
            problems.append(f"{source} → {target} 从 {allowed} 处涨到 {len(hits)} 处")
    if problems:
        print("\n门禁未通过：")
        for item in problems:
            print(f"  ✗ {item}")
        return 1
    print("\n门禁通过：没有新增跨层引用，已有的也没变多")
    return 0


if __name__ == "__main__":
    sys.exit(main())
