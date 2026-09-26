#!/bin/bash
# 安装/卸载 iMessage 桥接的 LaunchAgent。
#   ./install.sh            安装并启动
#   ./install.sh uninstall  停止并移除
#   ./install.sh restart    重启(改了脚本一般不用,它会自己热重载)
set -euo pipefail

LABEL="com.gqy.imessage-bridge"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"
CONFIG="$HOME/.gqy/config/imessage.json"
LOG="$HOME/.gqy/cache/logs/imessage-bridge.log"
# launchd 只接崩溃回溯;正常日志由脚本自己写 $LOG 并轮转
CRASH_LOG="$HOME/.gqy/cache/logs/imessage-bridge.crash.log"
DOMAIN="gui/$(id -u)"
# 解释器用 Python.app 里的本体,不经 /usr/bin/python3 转发
PYTHON="$(/usr/bin/python3 -c 'import os,sys;print(os.path.realpath(sys.executable))')"
APP_PYTHON="$(dirname "$PYTHON")/../Resources/Python.app/Contents/MacOS/Python"
if [ -x "$APP_PYTHON" ]; then
    PYTHON="$(cd "$(dirname "$APP_PYTHON")" && pwd)/Python"
fi
LAUNCHER="$HOME/.local/bin/gqy-imessage"

case "${1:-install}" in
uninstall)
    launchctl bootout "$DOMAIN/$LABEL" 2>/dev/null || true
    rm -f "$PLIST"
    echo "已卸载 $LABEL"
    exit 0
    ;;
restart)
    launchctl kickstart -k "$DOMAIN/$LABEL"
    echo "已重启 $LABEL"
    exit 0
    ;;
install) ;;
*)
    echo "用法: $0 [install|uninstall|restart]" >&2
    exit 2
    ;;
esac

mkdir -p "$HOME/Library/LaunchAgents" "$(dirname "$CONFIG")" "$(dirname "$LOG")"

if [ ! -f "$CONFIG" ]; then
    cp "$SCRIPT_DIR/config.example.json" "$CONFIG"
    # 模板里 enabled=true,先关掉,填好口令再打开
    /usr/bin/sed -i '' 's/"enabled": true/"enabled": false/' "$CONFIG"
    echo "已生成配置 $CONFIG(默认关闭)。填好 token(与 gqy 配置 platforms.connectors.imessage.token 相同)后把 enabled 改成 true"
fi

# 专用启动器:完全磁盘访问权限只授给它(见 launcher.c)。
# 内容不变就不替换,否则签名变了要重新授权。
mkdir -p "$(dirname "$LAUNCHER")"
# 输出文件名会写进签名标识,临时目录里也用固定文件名,保证重复编译结果一致
TMP_DIR="$(mktemp -d -t gqy-imessage)"
TMP_BIN="$TMP_DIR/gqy-imessage"
/usr/bin/cc -O2 -Wall \
    -DBRIDGE_PYTHON="\"$PYTHON\"" \
    -DBRIDGE_SCRIPT="\"$SCRIPT_DIR/imessage_bridge.py\"" \
    -o "$TMP_BIN" "$SCRIPT_DIR/launcher.c"
if [ -f "$LAUNCHER" ] && cmp -s "$TMP_BIN" "$LAUNCHER"; then
    rm -rf "$TMP_DIR"
else
    mv "$TMP_BIN" "$LAUNCHER"
    chmod 755 "$LAUNCHER"
    rm -rf "$TMP_DIR"
    echo "已编译启动器 $LAUNCHER(首次或有变化时需在「完全磁盘访问权限」里授权它)"
fi

cat >"$PLIST" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>$LABEL</string>
    <key>ProgramArguments</key>
    <array>
        <string>$LAUNCHER</string>
    </array>
    <key>EnvironmentVariables</key>
    <dict>
        <key>PATH</key>
        <string>/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin</string>
        <key>PYTHONUNBUFFERED</key>
        <string>1</string>
    </dict>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>ThrottleInterval</key>
    <integer>10</integer>
    <key>ProcessType</key>
    <string>Background</string>
    <key>StandardOutPath</key>
    <string>$CRASH_LOG</string>
    <key>StandardErrorPath</key>
    <string>$CRASH_LOG</string>
</dict>
</plist>
EOF

launchctl bootout "$DOMAIN/$LABEL" 2>/dev/null || true
# 刚 bootout 的任务可能还没收完,bootstrap 会报 5: Input/output error,稍等重试
for attempt in 1 2 3 4 5; do
    if launchctl bootstrap "$DOMAIN" "$PLIST" 2>/dev/null; then
        break
    fi
    if [ "$attempt" = 5 ]; then
        launchctl bootstrap "$DOMAIN" "$PLIST"
    fi
    sleep 1
done
echo "已启动 $LABEL,日志: $LOG"
