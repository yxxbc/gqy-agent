#!/usr/bin/env python3
"""把某个 Release 的版本号和各平台包的校验和写进 nix/release.json。

    python3 nix/update-release.py v0.6.0 path/to/SHA256SUMS

flake 的默认包按 release.json 下载 Releases 里编译好的包，用户不用本地编译。
发布流程在上传完 Release 后调用它，再把 release.json 提交回分支。
"""

import base64
import json
import sys
from pathlib import Path

# Nix 的 system 名 → Release 包里的平台名
TARGETS = {
    "x86_64-linux": "x86_64-unknown-linux-gnu",
    "aarch64-linux": "aarch64-unknown-linux-gnu",
    "aarch64-darwin": "aarch64-apple-darwin",
    "x86_64-darwin": "x86_64-apple-darwin",
}


def main() -> None:
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    tag, sums_path = sys.argv[1], Path(sys.argv[2])
    if not tag.startswith("v"):
        sys.exit(f"tag 应该以 v 开头：{tag}")

    sums = {}
    for line in sums_path.read_text().splitlines():
        if line.strip():
            digest, name = line.split()
            sums[name.lstrip("*")] = digest

    platforms = {}
    for system, target in TARGETS.items():
        name = f"gqy-{target}.tar.gz"
        digest = sums.get(name)
        if digest is None:
            sys.exit(f"SHA256SUMS 里没有 {name}")
        sri = "sha256-" + base64.b64encode(bytes.fromhex(digest)).decode()
        platforms[system] = {"target": target, "hash": sri}

    out = Path(__file__).with_name("release.json")
    data = {"tag": tag, "version": tag[1:], "platforms": platforms}
    out.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n")
    print(f"已写入 {out}（{tag}）")


if __name__ == "__main__":
    main()
