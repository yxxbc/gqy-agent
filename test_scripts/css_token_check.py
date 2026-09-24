#!/usr/bin/env python3
"""CSS token 门禁：新写的规则用 token，不写字面量字号与全局层级。

数两样东西（web/css/*.css）：
  - `font-size: <数字>px`：字号用 --fs-chat/ui/meta/micro（00-tokens.css）；
  - `z-index: <两位及以上数字>`：跨组件的层级用 --z-*，个位数的局部叠放不管。

存量不要求一次清零（标题、artifact 换算值、几何绑定的小字是有意保留的，见
docs/design/2026-09-24-webui-split.md），所以按文件对比基线：数量只许减少。
减少之后跑 --write-baseline 把基线收紧。

    python3 test_scripts/css_token_check.py                  # 门禁
    python3 test_scripts/css_token_check.py --write-baseline # 记录当前数量
"""
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CSS = ROOT / "web" / "css"
BASELINE = Path(__file__).resolve().parent / "css-token-baseline.json"
PATTERNS = {
    "font-size-px": re.compile(r"font-size:\s*\d+(?:\.\d+)?px"),
    "z-index-literal": re.compile(r"z-index:\s*-?\d{2,}"),
}


def count():
    rows = {}
    for path in sorted(CSS.glob("*.css")):
        text = re.sub(r"/\*.*?\*/", "", path.read_text(encoding="utf-8"), flags=re.S)
        rows[path.name] = {name: len(pattern.findall(text)) for name, pattern in PATTERNS.items()}
    return rows


def main():
    rows = count()
    if "--write-baseline" in sys.argv:
        BASELINE.write_text(json.dumps(rows, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        print(f"基线已写入 {BASELINE.relative_to(ROOT)}")
        return 0
    baseline = json.loads(BASELINE.read_text(encoding="utf-8"))
    problems = []
    for name, now in rows.items():
        was = baseline.get(name, {key: 0 for key in PATTERNS})
        for key, value in now.items():
            if value > was.get(key, 0):
                problems.append(f"{name}：{key} {was.get(key, 0)} → {value}（用 00-tokens.css 里的 token）")
    if problems:
        print("CSS token 门禁未通过：")
        for item in problems:
            print(f"  ✗ {item}")
        return 1
    total = {key: sum(row[key] for row in rows.values()) for key in PATTERNS}
    print(f"CSS token 门禁通过（存量：字面量字号 {total['font-size-px']}，字面量层级 {total['z-index-literal']}）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
