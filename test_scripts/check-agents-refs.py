#!/usr/bin/env python3
"""AGENTS.md 引用存在性门禁。

AGENTS.md 是每个会话开头整份读进上下文的规则书，里面写死了大量路径和符号
（`src/tools/descriptions/*.json`、`TURN_COLUMNS`、`gqy_executable()`……）。
代码搬家或删除后没人回头改它，下一个会话就会按一份过期地图干活——
`docs/plan-is-true/low-footprint.md` 就这样挂了很久。

只检查反引号里的两类东西：

  路径  含 `/` 或以常见扩展名结尾。先按仓库根解析（支持 glob），不行再按
        后缀匹配已跟踪文件（`rows.rs`、`config/plugin_catalog.rs` 这种省略写法）。
  符号  `snake_case()`、`SCREAMING_CASE`、`a::b`、`.method()`、含下划线的
        标识符。要求在 src/ 或 tests/ 里以整词出现。

命令、URL、占位符（含空格、`$`、`<`、`~`、`:` 前缀协议）一律跳过。
刻意提到的已删除之物登记在 INTENTIONAL_MISSING，写明理由。

只能抓「提到的东西没了」，抓不到「描述的流程变了」——后者靠 AGENTS.md §8 的对照表。
"""

import fnmatch
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TARGET = ROOT / "AGENTS.md"

# 刻意引用、本就不该存在的东西
INTENTIONAL_MISSING = {
    "packaging/": "§7.7 说明上游打包已删除、禁止恢复",
}

PATH_EXT = re.compile(r"\.(rs|md|py|sh|nix|json|ya?ml|toml|lock|txt)$")
SYMBOL = re.compile(r"^\.?[A-Za-z_][A-Za-z0-9_]*(::[A-Za-z_][A-Za-z0-9_]*)*(\(\))?$")


def tracked_files():
    # -z:不然中文文件名会被 git 转义成 "\346\226..."，全部误判为不存在
    out = subprocess.run(
        ["git", "ls-files", "-z"], capture_output=True, text=True, cwd=ROOT, check=True
    ).stdout
    files = [f for f in out.split("\0") if f]
    dirs = {str(Path(f).parent) + "/" for f in files}
    for f in list(dirs):
        parts = Path(f).parts
        for i in range(1, len(parts)):
            dirs.add("/".join(parts[:i]) + "/")
    return files, dirs


def is_skipped(token):
    return (
        any(ch in token for ch in " $<>~…")
        or token.startswith(("/", "-", "github:", "http"))
        or re.match(r"^[a-z]+:$", token) is not None  # kb: artifact: 前缀
    )


def path_exists(token, files, dirs):
    if token.endswith("/"):
        return token in dirs or any(d.endswith("/" + token) for d in dirs)
    if "*" in token:
        return any(fnmatch.fnmatch(f, token) for f in files)
    if token in files:
        return True
    return any(f.endswith("/" + token) for f in files)


def symbol_exists(token, sources):
    name = token.lstrip(".").removesuffix("()").split("::")[-1]
    return re.search(rf"\b{re.escape(name)}\b", sources) is not None


def looks_like_symbol(token):
    if not SYMBOL.match(token):
        return False
    bare = token.lstrip(".").removesuffix("()")
    return (
        token.endswith("()")
        or "::" in token
        or "_" in bare
        or (bare.isupper() and len(bare) >= 3)
    )


def main():
    files, dirs = tracked_files()
    sources = "\n".join(
        (ROOT / f).read_text(errors="ignore")
        for f in files
        if f.endswith(".rs") and f.startswith(("src/", "tests/"))
    )
    missing = []
    checked = 0
    for lineno, line in enumerate(TARGET.read_text().splitlines(), 1):
        for token in re.findall(r"`([^`\n]+)`", line):
            if token in INTENTIONAL_MISSING or is_skipped(token):
                continue
            if "/" in token or PATH_EXT.search(token):
                checked += 1
                if not path_exists(token, files, dirs):
                    missing.append((lineno, "路径", token))
            elif looks_like_symbol(token):
                checked += 1
                if not symbol_exists(token, sources):
                    missing.append((lineno, "符号", token))
    if missing:
        print("AGENTS.md 引用了仓库里已经不存在的东西：")
        for lineno, kind, token in missing:
            print(f"  AGENTS.md:{lineno}  {kind}  `{token}`")
        print("\n同一提交里更新 AGENTS.md；刻意引用已删除之物就登记进 INTENTIONAL_MISSING。")
        sys.exit(1)
    print(f"AGENTS.md 引用检查通过（{checked} 处路径/符号）")


if __name__ == "__main__":
    main()
