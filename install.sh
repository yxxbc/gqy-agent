#!/bin/sh
# 顾清影（gqy）一键安装脚本
#
#   curl -fsSL https://raw.githubusercontent.com/yxxbc/gqy-agent/gqy/install.sh | sh
#
# 从 GitHub Releases 下载云端编译好的包，校验 sha256，装到 ~/.local（不需要 root）。
#
# 参数（通过管道运行时写在 `sh -s --` 后面）：
#   --preview                 只播放安装界面、模拟下载进度，不联网也不安装
#   --help                    显示帮助
#
# 可以用环境变量调整：
#   GQY_VERSION=v0.6.0        装指定版本，默认最新
#   GQY_PREFIX=/opt/gqy       装到别的前缀，默认 ~/.local
#   GQY_DOWNLOAD_BASE=...     换下载地址（镜像），默认 https://github.com/yxxbc/gqy-agent/releases
#   GQY_FORCE=1               已经用 Nix 装过也照样装
#   GQY_PREVIEW=1             同 --preview
#   GQY_PLAIN=1               不画动画界面，只输出文字（日志、CI 里用）
#
# 界面：顶部固定一块 GQY 艺术字（雾蓝到酒红渐变，扫光和 TUI 开屏一样），下面是
# 当前步骤与小贴士，底部是进度条，整块原地刷新。不是终端、终端太窄或 TERM=dumb
# 时自动退回纯文字。

set -eu

REPO="yxxbc/gqy-agent"
VERSION="${GQY_VERSION:-latest}"
PREFIX="${GQY_PREFIX:-$HOME/.local}"
BASE="${GQY_DOWNLOAD_BASE:-https://github.com/$REPO/releases}"
PREVIEW="${GQY_PREVIEW:-}"

for arg in "$@"; do
  case "$arg" in
    --preview | -p) PREVIEW=1 ;;
    --help | -h)
      cat <<HELP
顾清影一键安装脚本

用法：
  curl -fsSL https://raw.githubusercontent.com/${REPO}/gqy/install.sh | sh
  curl -fsSL https://raw.githubusercontent.com/${REPO}/gqy/install.sh | sh -s -- --preview

参数：
  --preview   只播放安装界面、模拟下载进度，不联网也不安装
  --help      显示这段帮助

环境变量：
  GQY_VERSION=v0.6.0      装指定版本，默认最新
  GQY_PREFIX=/opt/gqy     装到别的前缀，默认 ~/.local
  GQY_DOWNLOAD_BASE=...   换下载地址（镜像）
  GQY_FORCE=1             已经用 Nix 装过也照样装
  GQY_PREVIEW=1           同 --preview
  GQY_PLAIN=1             不画动画界面，只输出文字
HELP
      exit 0
      ;;
    *) printf '顾清影安装脚本：不认识的参数 %s，用 --help 看用法。\n' "${arg}" >&2; exit 2 ;;
  esac
done

say() { printf '%s\n' "$*"; }

# 界面里显示的路径：家目录写成 ~，还太长就只留最后两级。界面的每一行都不能折行
short_path() {
  case "$1" in
    "$HOME"/*) p="~${1#"$HOME"}" ;;
    *) p="$1" ;;
  esac
  if [ "${#p}" -gt 32 ]; then
    p=".../$(basename "$(dirname "$p")")/$(basename "$p")"
  fi
  printf '%s' "$p"
}

# ───────────────────────────── 界面 ─────────────────────────────

UI=""
if [ -z "${GQY_PLAIN:-}" ] && [ -t 1 ] && [ "${TERM:-dumb}" != "dumb" ] && command -v awk >/dev/null 2>&1; then
  UI=1
fi
COLS=80
if [ -n "$UI" ]; then
  COLS="$( (stty size </dev/tty) 2>/dev/null | awk '{print $2}')"
  [ -n "$COLS" ] || COLS="$(tput cols 2>/dev/null || echo 80)"
  # 界面里最宽的一行（小贴士）约 56 列，再窄就会折行、原地刷新错位，退回纯文字
  [ "$COLS" -ge 60 ] 2>/dev/null || UI=""
fi
COLOR=256
case "${COLORTERM:-}" in truecolor | 24bit) COLOR=true ;; esac
# 帧间隔：支持小数秒的 sleep 用 0.1s，不支持的（少数 busybox）退成 1s
if sleep 0.01 2>/dev/null; then FRAME=0.1; else FRAME=1; fi

TICK=0
DRAWN=0
FRAME_LINES=13
STARTED="$(date +%s)"

# 画一帧：$1 步骤说明，$2 进度（0-100，-1 = 不确定），$3 已下载字节，$4 总字节（0 = 未知）
draw() {
  [ -n "$UI" ] || return 0
  now="$(date +%s)"
  if [ "$DRAWN" = 1 ]; then printf '\033[%dA' "$FRAME_LINES"; fi
  awk -v tick="$TICK" -v stage="$1" -v pct="$2" -v done_b="$3" -v total_b="$4" \
      -v secs="$((now - STARTED))" -v cols="$COLS" -v color="$COLOR" '
    function rgb(r, g, b) {
      if (color == "true") return sprintf("\033[38;2;%d;%d;%dm", r, g, b)
      return sprintf("\033[38;5;%dm", 16 + 36 * int(r / 255 * 5 + 0.5) + 6 * int(g / 255 * 5 + 0.5) + int(b / 255 * 5 + 0.5))
    }
    function mb(n) { return sprintf("%.1f", n / 1048576) }
    BEGIN {
      art[0] = ".BBBBBBa..BBBBBBa.BBa...BBa"
      art[1] = "BBbccccf.BBbcccBBaeBBa.BBbf"
      art[2] = "BBd..BBBaBBd...BBd.eBBBBbf."
      art[3] = "BBd...BBdBBdgg.BBd..eBBbf.."
      art[4] = "eBBBBBBbfeBBBBBBbf...BBd..."
      art[5] = ".ecccccf..ecchhcf....ecf..."
      glyph["B"] = "█"; glyph["a"] = "╗"; glyph["b"] = "╔"; glyph["c"] = "═"; glyph["d"] = "║"
      glyph["e"] = "╚"; glyph["f"] = "╝"; glyph["g"] = "▄"; glyph["h"] = "▀"; glyph["."] = " "
      w = 27; h = 6; pad = "  "
      # 扫光：25 帧扫过去，再停 15 帧
      cycle = tick % 40; glint = (cycle < 25) ? -4 + cycle * (w + 8) / 25 : -100
      print "\033[2K"
      for (y = 0; y < h; y++) {
        line = "\033[2K" pad
        for (x = 0; x < w; x++) {
          c = substr(art[y], x + 1, 1)
          if (c == ".") { line = line " "; continue }
          t = (x / (w - 1)) * 0.8 + (y / (h - 1)) * 0.2
          r = 174 + (227 - 174) * t; g = 189 + (140 - 189) * t; b = 232 + (154 - 232) * t
          d = x - glint; if (d < 0) d = -d
          if (d < 3) { k = (3 - d) / 3 * 0.75; r += (255 - r) * k; g += (255 - g) * k; b += (255 - b) * k }
          line = line rgb(r, g, b) glyph[c]
        }
        print line "\033[0m"
      }
      print "\033[2K" pad "\033[2m          A G E N T\033[0m"
      print "\033[2K"
      # 多字节字符一律整个存进数组再取，不用 substr 切：有的 awk 按字节、有的按字符数
      split("⠋|⠙|⠹|⠸|⠼|⠴|⠦|⠧|⠇|⠏", spins, "|")
      spin = (pct >= 100) ? "✓" : spins[tick % 10 + 1]
      print "\033[2K" pad rgb(174, 189, 232) spin "\033[0m " stage
      n = split("第一次打开 gqy 会进入新手引导，五步就能聊|没有 API key 也能先用免费额度试试|gqy dev 是开发模式，只留写代码需要的工具|gqy web 可以在手机、平板的浏览器里用|gqy -h 看全部命令，gqy config 改设置", tips, "|")
      print "\033[2K" pad "\033[2m小贴士：" tips[int(tick / 40) % n + 1] "\033[0m"
      print "\033[2K"
      # 进度条那一行：边距 2 + 条 + 2 + 数字（最长约 34 列），留足余量，折行会让原地刷新错位
      bar_w = cols - 40; if (bar_w > 44) bar_w = 44
      if (pct >= 0) {
        fill = int(bar_w * pct / 100 + 0.5); lit = ""; rest = ""
        for (i = 0; i < bar_w; i++) { if (i < fill) lit = lit "━"; else rest = rest "─" }
        bar = rgb(227, 140, 154) lit "\033[0m\033[2m" rest "\033[0m"
        info = sprintf("%3d%%", pct)
      } else {
        # 不知道总大小：一段亮块来回走
        pos = tick % (2 * (bar_w - 6)); if (pos > bar_w - 6) pos = 2 * (bar_w - 6) - pos
        bar = "\033[2m"
        for (i = 0; i < bar_w; i++) bar = bar ((i >= pos && i < pos + 6) ? "\033[0m" rgb(227, 140, 154) "━\033[0m\033[2m" : "─")
        bar = bar "\033[0m"; info = "  …"
      }
      if (done_b > 0) {
        info = info "  " mb(done_b)
        if (total_b > 0) info = info " / " mb(total_b)
        info = info " MB"
        if (secs > 0 && pct < 100) info = info " · " mb(done_b / secs) " MB/s"
      }
      print "\033[2K" pad bar "  " info
    }'
  DRAWN=1
  TICK=$((TICK + 1))
}

ui_start() {
  [ -n "$UI" ] || return 0
  printf '\033[?25l'
}

ui_end() {
  [ -n "$UI" ] || return 0
  printf '\033[?25h'
}

die() {
  ui_end
  printf '\n顾清影安装失败：%s\n' "$*" >&2
  exit 1
}

# ───────────────────────────── 预览 ─────────────────────────────

if [ -n "$PREVIEW" ]; then
  if [ -z "$UI" ]; then
    say "预览需要在终端里运行（当前输出不是终端、终端窄于 60 列、TERM=dumb，或设置了 GQY_PLAIN）。"
    exit 0
  fi
  trap 'ui_end' EXIT
  trap 'ui_end; exit 130' INT TERM
  ui_start
  total=$((27 * 1048576 + 318767))
  got=0
  STARTED=$(( $(date +%s) - 1 ))
  while [ "$got" -lt "$total" ]; do
    got=$((got + 180000 + (TICK % 7) * 60000))
    [ "$got" -gt "$total" ] && got=$total
    draw "正在下载顾清影 · 预览（不会真的下载）" $((got * 90 / total)) "$got" "$total"
    sleep "$FRAME"
  done
  for step in "正在校验 sha256:92" "正在解压:96" "正在安装到 $(short_path "$PREFIX"):99"; do
    i=0
    while [ $i -lt 8 ]; do
      draw "${step%:*}" "${step##*:}" "$total" "$total"
      sleep "$FRAME"; i=$((i + 1))
    done
  done
  draw "预览结束：没有下载，也没有安装任何东西" 100 "$total" "$total"
  ui_end
  exit 0
fi

# ───────────────────────────── 检查环境 ─────────────────────────────

# 已经用 Nix 装过就别再装一份，两份会在 PATH 里互相遮挡
if [ -z "${GQY_FORCE:-}" ]; then
  for nix_gqy in "$HOME/.nix-profile/bin/gqy" "/etc/profiles/per-user/${USER:-}/bin/gqy" /run/current-system/sw/bin/gqy; do
    if [ -x "$nix_gqy" ]; then
      die "已经用 Nix 装过顾清影（${nix_gqy}）。升级请用：nix profile upgrade gqy-agent
确实要再装一份到 ${PREFIX} 的话，设置 GQY_FORCE=1 再运行。"
    fi
  done
fi
if command -v nix >/dev/null 2>&1; then
  say "提示：检测到 Nix，推荐改用 nix profile install github:${REPO}/gqy 安装，升级和卸载更干净。"
  say ""
fi

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

if command -v curl >/dev/null 2>&1; then
  DOWNLOADER=curl
elif command -v wget >/dev/null 2>&1; then
  DOWNLOADER=wget
else
  die "需要 curl 或 wget 才能下载。"
fi

# 静默下载（进度由界面自己画）
fetch_quiet() {
  if [ "$DOWNLOADER" = curl ]; then
    curl -fsSL --retry 3 -o "$2" "$1"
  else
    wget -q -O "$2" "$1"
  fi
}

# 纯文字模式下用下载工具自己的进度条
fetch_plain() {
  if [ "$DOWNLOADER" = curl ]; then
    curl -fL --retry 3 --progress-bar -o "$2" "$1"
  else
    wget -q -O "$2" "$1"
  fi
}

# 跟完重定向之后的文件大小；拿不到就是空，界面改画不确定进度
remote_size() {
  [ "$DOWNLOADER" = curl ] || return 0
  curl -fsSIL --retry 2 "$1" 2>/dev/null | tr -d '\r' |
    awk 'tolower($1) == "content-length:" { n = $2 } END { if (n > 0) print n }'
}

file_size() {
  if [ -f "$1" ]; then wc -c <"$1" | tr -d ' '; else echo 0; fi
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
fetch_pid=""
cleanup() {
  [ -n "$fetch_pid" ] && kill "$fetch_pid" 2>/dev/null || true
  ui_end
  rm -rf "$tmp"
}
trap 'cleanup' EXIT
trap 'cleanup; exit 130' INT TERM

archive="$tmp/$name.tar.gz"
label="正在下载顾清影 · ${target} · ${VERSION}"
fail_hint="下载失败：${url}
如果网络访问 GitHub 很慢，可以设置 GQY_DOWNLOAD_BASE 换一个下载地址。"

# ───────────────────────────── 下载 ─────────────────────────────

if [ -n "$UI" ]; then
  ui_start
  draw "正在连接 GitHub …" -1 0 0
  total="$(remote_size "$url")"
  total="${total:-0}"
  STARTED="$(date +%s)"
  fetch_quiet "$url" "$archive" &
  fetch_pid=$!
  while kill -0 "$fetch_pid" 2>/dev/null; do
    got="$(file_size "$archive")"
    if [ "$total" -gt 0 ]; then
      draw "$label" $((got * 90 / total)) "$got" "$total"
    else
      draw "$label" -1 "$got" 0
    fi
    sleep "$FRAME"
  done
  wait "$fetch_pid" || { fetch_pid=""; die "$fail_hint"; }
  fetch_pid=""
  got="$(file_size "$archive")"
  [ "$total" -gt 0 ] || total="$got"
  draw "正在校验 sha256" 92 "$got" "$total"
  fetch_quiet "$url.sha256" "$archive.sha256" || die "校验文件下载失败：${url}.sha256"
else
  say "正在下载顾清影（${target}，版本：${VERSION}）"
  fetch_plain "$url" "$archive" || die "$fail_hint"
  fetch_plain "$url.sha256" "$archive.sha256" || die "校验文件下载失败：${url}.sha256"
  got="$(file_size "$archive")"
  total="$got"
fi

expected="$(awk '{print $1}' "$archive.sha256")"
actual="$(sha256_of "$archive")"
[ -n "$expected" ] && [ "$expected" = "$actual" ] || die "文件校验不通过，可能下载不完整，请重新运行一次。"

draw "正在解压" 96 "$got" "$total"
tar -xzf "$archive" -C "$tmp"
src="$tmp/$name"
[ -x "$src/bin/gqy" ] || die "安装包里没有找到 gqy。"

# ───────────────────────────── 安装 ─────────────────────────────

draw "正在安装到 $(short_path "$PREFIX")" 99 "$got" "$total"
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

draw "装好了" 100 "$got" "$total"
ui_end

say ""
say "装好了：${PREFIX}/bin/gqy"
"$PREFIX/bin/gqy" --version 2>/dev/null || true

case ":$PATH:" in
  *":$PREFIX/bin:"*) ;;
  *)
    say ""
    say "${PREFIX}/bin 还不在 PATH 里。把下面这行加到 ~/.zshrc 或 ~/.bashrc，再重新打开终端："
    say "  export PATH=\"${PREFIX}/bin:\$PATH\""
    ;;
esac

say ""
say "接下来："
say "  gqy        打开终端界面。第一次会进入新手引导，五步就能聊"
say "  gqy web    在浏览器里用（同一 Wi-Fi 的手机、平板也能打开）"
say "  gqy -h     查看全部命令"
