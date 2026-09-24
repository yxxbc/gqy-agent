#!/usr/bin/env python3
"""WebUI 模块的依赖方向门禁。

层序（只能从左往右依赖）：

    core → state → widgets → features → app.js（入口）

- 同层内：core / state / widgets 可以互相引用；同一个 feature 内部随意。
- feature 之间默认不许互相引用。确有需要的边写进 `web-deps.json` 的
  `feature_edges`，每条带理由——新增一条边就会出现在 diff 里。
- `web/` 根目录下除 app.js 以外的文件是拆分前的旧件（`window.GqyXxx` 形式），
  迁移完成前不参与检查，只报个数。
- 白名单里用不上的边报警告：边删了白名单没跟着删，它就会悄悄放行下一次。

设计见 docs/design/2026-09-24-webui-split.md §3。引用路径是否存在由 Rust 测试
`module_imports_resolve_to_embedded_assets` 负责，这里只管方向。

    python3 test_scripts/web_dep_check.py
"""
import json
import posixpath
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WEB = ROOT / "web"
CONFIG = Path(__file__).resolve().parent / "web-deps.json"

LAYERS = ["core", "state", "widgets", "features"]
ENTRY = "app.js"

IMPORT = re.compile(
    r"""(?:\bfrom\s*|\bimport\s*\(?\s*)(["'])(\.{1,2}/[^"']+|/[^/"'][^"']*)\1"""
)


def layer_of(rel):
    """返回 (层名, feature 名)。旧件返回 ("legacy", None)。"""
    parts = rel.split("/")
    if rel == ENTRY:
        return "entry", None
    if parts[0] in LAYERS and len(parts) > 1:
        feature = parts[1].removesuffix(".js") if parts[0] == "features" else None
        return parts[0], feature
    return "legacy", None


def rank(layer):
    return {"core": 0, "state": 1, "widgets": 2, "features": 3, "entry": 4}.get(layer)


def resolve(source_rel, specifier):
    if specifier.startswith("/"):
        return specifier.lstrip("/")
    return posixpath.normpath(posixpath.join(posixpath.dirname(source_rel), specifier))


def main():
    config = json.loads(CONFIG.read_text(encoding="utf-8"))
    allowed = {
        (edge["from"], edge["to"]) for edge in config.get("feature_edges", [])
    }
    used = set()
    problems = []
    legacy = 0

    for path in sorted(WEB.rglob("*.js")):
        rel = path.relative_to(WEB).as_posix()
        if rel.startswith("vendor/"):
            continue
        layer, feature = layer_of(rel)
        if layer == "legacy":
            legacy += 1
            continue
        source = path.read_text(encoding="utf-8")
        for match in IMPORT.finditer(source):
            target_rel = resolve(rel, match.group(2))
            target_layer, target_feature = layer_of(target_rel)
            where = f"{rel} → {target_rel}"
            if target_layer == "legacy":
                problems.append(f"{where}：新模块不许依赖未迁移的旧件，先把它迁进分层")
            elif target_layer == "entry":
                problems.append(f"{where}：任何模块都不许 import 入口 app.js")
            elif rank(target_layer) > rank(layer):
                problems.append(f"{where}：{layer} 不能依赖更靠后的 {target_layer}")
            elif layer == "features" and target_layer == "features" and feature != target_feature:
                edge = (feature, target_feature)
                if edge in allowed:
                    used.add(edge)
                else:
                    problems.append(
                        f"{where}：feature 之间的依赖要在 web-deps.json 的 feature_edges 里声明"
                    )

    for edge in sorted(allowed - used):
        print(f"  ! 白名单里的边没人用了：{edge[0]} → {edge[1]}（删掉它）")

    if problems:
        print("WebUI 依赖方向门禁未通过：")
        for item in problems:
            print(f"  ✗ {item}")
        return 1
    print(f"WebUI 依赖方向通过（未迁移的旧件 {legacy} 个）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
