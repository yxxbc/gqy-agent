#!/bin/sh
# 顾清影（gqy）一键安装脚本
#
#   curl -fsSL https://raw.githubusercontent.com/yxxbc/gqy-agent/gqy/install.sh | sh
#
# 从 GitHub Releases 下载云端编译好的包，校验 sha256，装到 ~/.local（不需要 root）。
# 可以用环境变量调整：
#   GQY_VERSION=v0.6.0        装指定版本，默认最新
#   GQY_PREFIX=/opt/gqy       装到别的前缀，默认 ~/.local
#   GQY_DOWNLOAD_BASE=...     换下载地址（镜像），默认 https://github.com/yxxbc/gqy-agent/releases
#   GQY_FORCE=1               已经用 Nix 装过也照样装

set -eu

REPO="yxxbc/gqy-agent"
VERSION="${GQY_VERSION:-latest}"
PREFIX="${GQY_PREFIX:-$HOME/.local}"
BASE="${GQY_DOWNLOAD_BASE:-https://github.com/$REPO/releases}"

say() { printf '%s\n' "$*"; }
die() { printf '\n顾清影安装失败：%s\n' "$*" >&2; exit 1; }

# ── 已经用 Nix 装过就别再装一份，两份会在 PATH 里互相遮挡 ──
if [ -z "${GQY_FORCE:-}" ]; then
  for nix_gqy in "$HOME/.nix-profile/bin/gqy" "/etc/profiles/per-user/${USER:-}/bin/gqy" /run/current-system/sw/bin/gqy; do
    if [ -x "$nix_gqy" ]; then
      die "已经用 Nix 装过顾清影（${nix_gqy}）。升级请用：nix profile upgrade gqy-agent
确实要再装一份到 $PREFIX 的话，设置 GQY_FORCE=1 再运行。"
    fi
  done
fi
if command -v nix >/dev/null 2>&1; then
  say "提示：检测到 Nix，推荐改用 nix profile install github:${REPO}/gqy 安装，升级和卸载更干净。"
  say ""
fi

# ── 看看是什么电脑 ──
os="$(uname -s)"
arch="$(uname -m)"
case "$os" in
  Linux) os_part="unknown-linux-gnu" ;;
  Darwin) os_part="apple-darwin" ;;
  *) die "暂时只支持 Linux 和 macOS，当前系统是 ${os}。" ;;
esac
case "$arch" in
  x86_64 | amd64) arch_part="x86_64" ;;
  aarch64 | arm64) arch_part="aarch64" ;;
  *) die "暂时不支持这个 CPU 架构：${arch}。" ;;
esac
# 在 Apple 芯片的 Mac 上用 Rosetta 跑的终端，也装原生 ARM 版
if [ "$os" = "Darwin" ] && [ "$arch_part" = "x86_64" ]; then
  if [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" = "1" ]; then
    arch_part="aarch64"
  fi
fi
target="${arch_part}-${os_part}"
name="gqy-${target}"

if [ "$VERSION" = "latest" ]; then
  url="$BASE/latest/download/$name.tar.gz"
else
  url="$BASE/download/$VERSION/$name.tar.gz"
fi

fetch() {
  if command -v curl >/dev/null 2>&1; then
    curl -fL --retry 3 --progress-bar -o "$2" "$1"
  elif command -v wget >/dev/null 2>&1; then
    wget -q -O "$2" "$1"
  else
    die "需要 curl 或 wget 才能下载。"
  fi
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    die "需要 sha256sum 或 shasum 来校验下载的文件。"
  fi
}

tmp="$(mktemp -d 2>/dev/null || mktemp -d -t gqy)"
trap 'rm -rf "$tmp"' EXIT INT TERM

say "正在下载顾清影（${target}，版本：${VERSION}）"
fetch "$url" "$tmp/$name.tar.gz" || die "下载失败：$url
如果网络访问 GitHub 很慢，可以设置 GQY_DOWNLOAD_BASE 换一个下载地址。"
fetch "$url.sha256" "$tmp/$name.tar.gz.sha256" || die "校验文件下载失败：$url.sha256"

expected="$(awk '{print $1}' "$tmp/$name.tar.gz.sha256")"
actual="$(sha256_of "$tmp/$name.tar.gz")"
[ -n "$expected" ] && [ "$expected" = "$actual" ] || die "文件校验不通过，可能下载不完整，请重新运行一次。"

tar -xzf "$tmp/$name.tar.gz" -C "$tmp"
src="$tmp/$name"
[ -x "$src/bin/gqy" ] || die "安装包里没有找到 gqy。"

# ── 装进 ~/.local（或 GQY_PREFIX） ──
mkdir -p "$PREFIX/bin" "$PREFIX/share" "$PREFIX/lib" || die "没法写入 ${PREFIX}，可以用 GQY_PREFIX 换一个目录。"

# 先写到临时文件再改名，正在运行的 gqy 不会被写坏
cp "$src/bin/gqy" "$PREFIX/bin/.gqy.new"
chmod 0755 "$PREFIX/bin/.gqy.new"
mv -f "$PREFIX/bin/.gqy.new" "$PREFIX/bin/gqy"

cp -R "$src/share/." "$PREFIX/share/"
if [ -d "$src/lib/gqy" ]; then
  mkdir -p "$PREFIX/lib/gqy"
  cp -RP "$src/lib/gqy/." "$PREFIX/lib/gqy/"
fi

if [ "$os" = "Darwin" ] && command -v xattr >/dev/null 2>&1; then
  xattr -dr com.apple.quarantine "$PREFIX/bin/gqy" "$PREFIX/lib/gqy" 2>/dev/null || true
fi

say ""
say "装好了：$PREFIX/bin/gqy"
"$PREFIX/bin/gqy" --version 2>/dev/null || true

case ":$PATH:" in
  *":$PREFIX/bin:"*) ;;
  *)
    say ""
    say "$PREFIX/bin 还不在 PATH 里。把下面这行加到 ~/.zshrc 或 ~/.bashrc，再重新打开终端："
    say "  export PATH=\"$PREFIX/bin:\$PATH\""
    ;;
esac

say ""
say "接下来："
say "  gqy init           第一次用，先初始化"
say "  gqy daemon start   启动后台"
say "  gqy                打开终端界面，和她说话"
