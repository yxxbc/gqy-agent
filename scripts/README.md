# scripts/

不编译进 gqy 二进制、独立运行的集成。它们通过 `gqy ask` 等公开命令与本体交互，gqy 本体不依赖它们。

| 目录 | 作用 |
|---|---|
| `imessage/` | iMessage 双向桥接（LaunchAgent 守护进程）：`imessage_bridge.py` 收发消息，`imessage_commands.py` 处理手机上的 `/new` `/topic` `/model` `/pause` 等指令。见其 README 与 `docs/wiki/13-QQ与通讯平台.md`。改成 gqy 原生平台的方案稿在 `docs/design/2026-09-26-imessage-platform.md`，尚未施工 |

**不要挪动或改名这里的目录。** `imessage/install.sh` 会把桥接脚本的绝对路径编译进启动器，路径一变，已安装的桥接就会失效，重装后还要手动重新授予「完全磁盘访问权限」。

别和这几个目录混淆：`src/scripts/`（随包分发的人格脚本资源）、`docs/scripts/`（那些脚本的文档）、`test_scripts/`（门禁与开发工具）。
