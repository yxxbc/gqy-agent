#!/usr/bin/env python3
"""CHANGELOG.md 与发版的机械操作。发布工作流和 CI 都调它，本地也能直接跑。

    release.py check                    检查 CHANGELOG 格式（CI 用）
    release.py notes X.Y.Z              打印该版本那一段的正文（Release 正文用）
    release.py prepare X.Y.Z [日期]      定稿：[Unreleased] → [X.Y.Z] - 日期，
                                        同步 Cargo.toml / Cargo.lock / README 版本徽章

格式约定写在 CHANGELOG.md 开头。`<!-- legacy` 那行以下是 git-cliff 时代的
历史记录，不检查也不改。只用标准库，CI 上不装依赖。
"""

import datetime
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CHANGELOG = ROOT / "CHANGELOG.md"
UNRELEASED = "## [Unreleased]"
LEGACY_MARK = "<!-- legacy"
SECTIONS = ["Added", "Changed", "Deprecated", "Removed", "Fixed", "Security"]
VERSION_RE = re.compile(r"^\d+\.\d+\.\d+$")
HEADER_RE = re.compile(r"^## \[(\d+\.\d+\.\d+)\] - \d{4}-\d{2}-\d{2}$")


def fail(message):
    print(f"release.py: {message}", file=sys.stderr)
    sys.exit(1)


def read_lines():
    return CHANGELOG.read_text(encoding="utf-8").split("\n")


def checked_region(lines):
    """legacy 标记之前的部分（行号上界）。"""
    for index, line in enumerate(lines):
        if line.startswith(LEGACY_MARK):
            return index
    return len(lines)


def version_blocks(lines):
    """[(标题行号, 结束行号)]，只看受检区。"""
    end = checked_region(lines)
    starts = [i for i in range(end) if lines[i].startswith("## ")]
    return [(start, (starts + [end])[k + 1]) for k, start in enumerate(starts)]


def check_block(lines, start, end):
    errors = []
    header = lines[start]
    if header != UNRELEASED and not HEADER_RE.match(header):
        errors.append(f"第 {start + 1} 行：版本标题应为 `{UNRELEASED}` 或 `## [X.Y.Z] - YYYY-MM-DD`，实为 `{header}`")
    seen = []
    in_section = False
    for i in range(start + 1, end):
        line = lines[i]
        if line.startswith("### "):
            name = line[4:].strip()
            if name not in SECTIONS:
                errors.append(f"第 {i + 1} 行：小节只能是 {' / '.join(SECTIONS)}，实为 `{name}`")
            elif name in seen:
                errors.append(f"第 {i + 1} 行：`### {name}` 重复")
            elif seen and SECTIONS.index(name) < SECTIONS.index(seen[-1]):
                errors.append(f"第 {i + 1} 行：`### {name}` 应排在 `### {seen[-1]}` 之前")
            else:
                seen.append(name)
            in_section = True
        elif line.startswith("#"):
            errors.append(f"第 {i + 1} 行：版本段里不能有 `{line.split(' ')[0]}` 级标题")
        elif in_section and line.strip() and not line.startswith(("- ", "  ")):
            errors.append(f"第 {i + 1} 行：小节里每条以 `- ` 开头（续行缩进两格）")
    return errors


def has_entries(lines, start, end):
    return any(line.startswith("- ") for line in lines[start + 1:end])


def cmd_check():
    lines = read_lines()
    blocks = version_blocks(lines)
    errors = []
    if not blocks or lines[blocks[0][0]] != UNRELEASED:
        errors.append(f"第一个版本段必须是 `{UNRELEASED}`")
    if sum(1 for start, _ in blocks if lines[start] == UNRELEASED) > 1:
        errors.append(f"`{UNRELEASED}` 只能有一个")
    for start, end in blocks:
        errors.extend(check_block(lines, start, end))
    if errors:
        fail("CHANGELOG.md 格式不对：\n  " + "\n  ".join(errors))
    print(f"CHANGELOG.md ok（{len(blocks)} 个版本段）")


def find_version(lines, version):
    for start, end in version_blocks(lines):
        match = HEADER_RE.match(lines[start])
        if match and match.group(1) == version:
            return start, end
    return None


def find_any_version(lines, version):
    """全文件找，含 legacy 区——重新发布旧 tag 时也要取得到正文。"""
    starts = [i for i, line in enumerate(lines) if line.startswith("## ") or line.startswith(LEGACY_MARK)]
    for k, start in enumerate(starts):
        match = HEADER_RE.match(lines[start])
        if match and match.group(1) == version:
            return start, (starts + [len(lines)])[k + 1]
    return None


def cmd_notes(version):
    lines = read_lines()
    found = find_any_version(lines, version)
    if not found:
        fail(f"CHANGELOG.md 里没有 `## [{version}] - 日期` 这一段。发版要用 release 工作流，或先跑 prepare")
    start, end = found
    body = "\n".join(lines[start + 1:end]).strip()
    if not body:
        fail(f"[{version}] 这一段是空的")
    print(body)


def replace_once(path, pattern, replacement, what):
    text = path.read_text(encoding="utf-8")
    new, count = re.subn(pattern, replacement, text, count=1, flags=re.MULTILINE)
    if count != 1:
        fail(f"{path.name} 里找不到{what}")
    path.write_text(new, encoding="utf-8")


def cmd_prepare(version, date):
    cmd_check()
    lines = read_lines()
    if find_version(lines, version):
        fail(f"CHANGELOG.md 里已经有 [{version}]")
    start, end = version_blocks(lines)[0]
    if not has_entries(lines, start, end):
        fail("[Unreleased] 里没有任何条目，没东西可发")
    lines[start:start + 1] = [UNRELEASED, "", f"## [{version}] - {date}"]
    CHANGELOG.write_text("\n".join(lines), encoding="utf-8")

    replace_once(ROOT / "Cargo.toml", r'^version = "[^"]+"', f'version = "{version}"', " [package] 的 version")
    # Cargo.lock 里本包那一条：name = "gqy" 紧跟 version 行。不改的话 --locked 编译直接失败。
    replace_once(
        ROOT / "Cargo.lock",
        r'^(name = "gqy"\nversion = )"[^"]+"',
        rf'\g<1>"{version}"',
        ' name = "gqy" 的 version',
    )
    replace_once(ROOT / "README.md", r"badge/version-[0-9.]+-", f"badge/version-{version}-", "版本徽章")
    print(f"已定稿 {version}（{date}）：CHANGELOG.md、Cargo.toml、Cargo.lock、README.md")


def main(argv):
    if argv == ["check"]:
        return cmd_check()
    if len(argv) == 2 and argv[0] == "notes":
        return cmd_notes(argv[1].removeprefix("v"))
    if len(argv) in (2, 3) and argv[0] == "prepare":
        version = argv[1].removeprefix("v")
        if not VERSION_RE.match(version):
            fail(f"版本号应为 X.Y.Z，实为 `{argv[1]}`")
        date = argv[2] if len(argv) == 3 else datetime.date.today().isoformat()
        return cmd_prepare(version, date)
    fail(__doc__.strip())


if __name__ == "__main__":
    main(sys.argv[1:])
