#!/bin/sh
# 从 install.sh 装的顾清影换成 Nix 安装。
#
#   curl -fsSL https://raw.githubusercontent.com/yxxbc/gqy-agent/gqy/nix/migrate.sh | sh
#   sh nix/migrate.sh --dry-run      只看会做什么，不动任何东西
#
# 步骤：先用 Nix 装好新版本（失败就什么都不动）→ 停掉旧的后台 → 把 install.sh
# 放进前缀里的程序文件挪到 ~/.gqy/bin-backup/ 下备份（不删除）。
# 你的数据（~/.gqy 里的配置、会话、记忆、知识库）两边共用，不用迁移，也不会被碰。
#
# 可以用环境变量调整：
#   GQY_PREFIX=/opt/gqy    当初 install.sh 装到的前缀，默认 ~/.local
#   GQY_FLAKE=...          Nix 安装来源，默认 github:yxxbc/gqy-agent/gqy

set -eu

PREFIX="${GQY_PREFIX:-$HOME/.local}"
FLAKE="${GQY_FLAKE:-github:yxxbc/gqy-agent/gqy}"
GQY_HOME_DIR="${GQY_HOME:-$HOME/.gqy}"
DRY_RUN=0
[ "${1:-}" = "--dry-run" ] && DRY_RUN=1

say() { printf '%s\n' "$*"; }
die() { printf '\n迁移失败：%s\n' "$*" >&2; exit 1; }
run() {
  if [ "$DRY_RUN" = 1 ]; then say "  [演练] $*"; else "$@"; fi
}

command -v nix >/dev/null 2>&1 || die "没有找到 nix。先按 https://nixos.org/download/ 装好 Nix（并开启 flakes），旧的安装一点没动。"

# install.sh 放进前缀的东西，只认这几项；前缀下别的文件（包括老版本留在
# ~/.local/share/gqy 里的数据）一概不碰
old_items=""
for item in bin/gqy lib/gqy share/gqy/fonts share/gqy/models share/gqy/scripts share/gqy/default-kb share/licenses/gqy; do
  if [ -e "$PREFIX/$item" ] || [ -L "$PREFIX/$item" ]; then
    old_items="$old_items $item"
  fi
done

# ── 1. 先用 Nix 装好 ──
nix_bin="$HOME/.nix-profile/bin/gqy"
if [ -x "$nix_bin" ]; then
  say "Nix 里已经装了 gqy：$nix_bin"
else
  say "用 Nix 安装：$FLAKE"
  run nix profile install "$FLAKE" || die "Nix 安装失败，旧的安装一点没动。"
fi

if [ -z "$old_items" ]; then
  say ""
  say "$PREFIX 下没有找到 install.sh 装的文件，不用清理。"
else
  # ── 2. 停掉旧版本的后台，免得它还占着旧文件 ──
  if [ -x "$PREFIX/bin/gqy" ]; then
    say "停止旧版本的后台（没在运行也没关系）"
    run "$PREFIX/bin/gqy" daemon stop || true
  fi

  # ── 3. 挪去备份，不直接删 ──
  backup="$GQY_HOME_DIR/bin-backup/install-sh-$(date +%Y%m%d-%H%M%S)"
  say "把旧文件挪到 $backup"
  for item in $old_items; do
    run mkdir -p "$backup/$(dirname "$item")"
    run mv "$PREFIX/$item" "$backup/$item"
    say "  $PREFIX/$item"
  done
  # share/gqy 挪空了就顺手删掉空目录；不空说明还有别的数据，留着
  run rmdir "$PREFIX/share/gqy" 2>/dev/null || true
fi

# ── 4. 看看终端里的 gqy 现在是哪一个 ──
say ""
hash -r 2>/dev/null || true
current="$(command -v gqy 2>/dev/null || true)"
case "$current" in
  "$HOME/.nix-profile/bin/gqy" | /nix/* | /etc/profiles/*) say "完成。终端里的 gqy 现在是 Nix 装的：$current" ;;
  "") say "完成。重新打开终端后 gqy 就能用了（~/.nix-profile/bin 要在 PATH 里）。" ;;
  *)
    say "注意：终端里的 gqy 还是 ${current}，它排在 Nix 版前面。"
    case "$current" in
      "$HOME/.cargo/bin/gqy") say "这是 cargo 从源码装的开发版；不需要了就运行：cargo uninstall gqy" ;;
      *) say "不需要了就删掉它，或者调整 PATH 让 ~/.nix-profile/bin 排在前面。" ;;
    esac
    ;;
esac

# 自己写过绝对路径的配置不会自动改，提醒一下
for file in "$GQY_HOME_DIR/config/imessage.json" "$GQY_HOME_DIR/config/config.jsonc"; do
  if [ -f "$file" ] && grep -q "$PREFIX/bin/gqy" "$file" 2>/dev/null; then
    say "注意：$file 里还写着 $PREFIX/bin/gqy，改成 gqy 或 ~/.nix-profile/bin/gqy。"
  fi
done

say ""
say "以后升级：nix profile upgrade gqy-agent"
say "想退回：nix profile remove gqy-agent，再把备份目录里的文件挪回 ${PREFIX}。"
