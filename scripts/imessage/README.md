# iMessage 连接器

让 顾清影 在 iMessage 上和名单里的联系人私聊。连接器只做 I/O，其余都在 gqy daemon 里：

```
LaunchAgent → gqy-imessage 启动器（持有完全磁盘访问权限，一次授权）
  └ imessage_bridge.py（连接器）
      ├ 读 ~/Library/Messages/chat.db 的私聊新消息 → event 帧
      ├ 收 daemon 的 send 帧 → osascript → 「信息」发出去 → send_result
      └ 本机 WebSocket：ws://127.0.0.1:8300/api/connector/ws?platform=imessage（带口令）
gqy daemon（platforms.connectors.imessage）
  └ 联系人名单、会话与话题、指令、拆气泡、模型、记忆、语音
```

协议是通用的 `gqy-connector/1`（`docs/design/2026-09-26-connector-protocol.md`，帧定义在 `src/platforms/connector/protocol.rs`）。以后接别的聊天软件，照这个协议再写一个连接器即可。

## 手机上的快捷指令

在聊天里直接发送，daemon 当场回复，不经过模型：

| 指令 | 作用 |
|---|---|
| `/new` | 开一个新话题（新会话，旧话题都保留，沿用当前话题的模型） |
| `/topics` | 列出所有话题：轮数、最近一次时间、最后一句的开头 |
| `/topic 2` | 切换到第 2 个话题 |
| `/model` | 看可选模型（带序号，标出当前） |
| `/model 5` 或 `/model 名字` | 这个话题换模型；`/model default` 恢复默认 |
| `/pause` / `/resume` | 暂停 / 恢复回复。暂停期间的消息直接跳过，恢复后不会补回 |
| `/help` | 显示指令说明 |

话题 1 是 `imessage-<联系人名>` 会话，之后是 `imessage-<联系人名>-2`、`-3`……（和旧桥接同名，历史直接接上）。旧桥接记在 `~/.gqy/state/imessage-contacts.json` 里的当前话题、模型、暂停状态，第一次经连接器收到这个联系人的消息时自动搬过去。

## 前提

- 本用户的 Messages 已登录 顾清影 的 Apple ID（Messages → 设置 → iMessage，可以和系统 iCloud 账号不同）。
- 该 Apple ID 已关闭「联系人密钥验证」，否则 iCloud 与 iMessage 账号不一致时无法登录。
- gqy daemon 在运行。

## 安装

1. 在 gqy 配置（`~/.gqy/config/config.jsonc`）里打开这个平台，写上联系人和口令：

   ```jsonc
   "platforms": {
     "connectors": {
       "imessage": {
         "enabled": true,
         "token": "一串随机字符",
         "contacts": [
           { "name": "me", "handles": ["+8613800000000", "me@icloud.com"], "owner": true }
         ]
       }
     }
   }
   ```

   口令可以用 `openssl rand -hex 24` 生成。改完 `gqy reload`。
2. 装连接器：`scripts/imessage/install.sh`。首次会生成 `~/.gqy/config/imessage.json`（默认关闭），填上同一个 `token`，把 `enabled` 改成 `true`，保存即生效。

### 授权

1. **完全磁盘访问**：系统设置 → 隐私与安全性 → 完全磁盘访问权限 → `+`，加入 `~/.local/bin/gqy-imessage`（`install.sh` 编译出的专用启动器），脚本约一分钟内自动重试生效。没授权时日志会打 `cannot read … Grant Full Disk Access to …`。
   为什么不直接授权 Python 或 gqy：Xcode 自带的 Python 授权会落到整个 Xcode 上；gqy 每次编译签名都变，每次升级都要重新授权。启动器只能拉起本脚本（路径编译时写死），内容不变就不替换，授权不失效。所以脚本路径不能挪（AGENTS.md §4.5）。
2. **自动化**：第一次回复时系统会弹「gqy-imessage 想要控制 信息」，点允许。

## gqy 配置 `platforms.connectors.imessage`

| 键 | 说明 | 默认 |
|---|---|---|
| `enabled` | 总开关 | `false` |
| `token` | 连接器口令，必填。空口令一律拒绝（沙盒里的成员会话也能连本机端口） | 空 |
| `contacts` | `[{name, handles, owner}]`。`name` 决定会话名；`handles` 是手机号或邮箱，同一人的多个账号合成一个对话，11 位国内手机号自动补 `+86`；`owner: true` 是你本人，记忆与终端 / WebUI 共享。名单外的人发来的消息不回 | `[]` |
| `owner_host_tools` | 你本人的对话能不能用宿主工具（跑命令、读写文件）。默认关：手机丢了或账号被盗时，别人不能借聊天窗口在电脑上执行命令 | `false` |
| `max_bubbles` | 一条回复最多拆成几条。段落超过上限时，相邻段落按长度均衡合并 | `6` |
| `bubble_pause_seconds` | 连发多条时每条之前停顿的上限（秒），按长度停 0.5 秒到这个上限，像在打字 | `2` |
| `memory_write_enabled` | 这个平台的对话能不能写记忆 | `true` |

## 连接器配置 `~/.gqy/config/imessage.json`

| 键 | 说明 | 默认 |
|---|---|---|
| `enabled` | 连接器开关。关闭期间到达的消息直接跳过，重新打开不会补回 | `false` |
| `url` | daemon 的连接器入口（改过 web 端口时跟着改） | `ws://127.0.0.1:8300/api/connector/ws?platform=imessage` |
| `token` | 与 gqy 配置里的 `token` 相同 | 空 |
| `poll_seconds` | 检查 chat.db 变化的间隔 | `2` |
| `max_backlog_minutes` | 连接器停机后重启，只补这么久以内的消息 | `30` |

## 运维

| 操作 | 命令 |
|---|---|
| 看日志 | `tail -f ~/.gqy/cache/logs/imessage-bridge.log`（自动轮转，总量封顶约 6 MB；意外崩溃的回溯在 `imessage-bridge.crash.log`）；daemon 那边看 `gqy::platform` |
| 暂停 | 手机上发 `/pause`，或连接器配置 `enabled: false` |
| 彻底停止并卸载 | `scripts/imessage/install.sh uninstall` |
| 重启 | `scripts/imessage/install.sh restart` |
| 看会话 | `gqy session list` 里的 `imessage-<name>` |

改 `imessage_bridge.py` 后，连接器在空闲时（没有等确认的消息）自动用新代码重载。改出语法错误时不重载，日志里会报错并继续跑旧代码。

读取进度记在 `~/.gqy/state/imessage-bridge.json`，daemon 处理完回确认才推进：daemon 重启或断线时，没确认的消息重连后重发，不会丢。

## 空间占用（都不用手动清理）

| 东西 | 在哪 | 怎么清 |
|---|---|---|
| 连接器日志 | `~/.gqy/cache/logs/imessage-bridge.log*` | 自动轮转，封顶约 6 MB |
| 待发附件的暂存副本 | `~/Library/Messages/.gqy-send-staging/`（Messages 沙盒只能读这附近） | 每次发送时自动删掉一小时前的 |
| 收到的 HEIC 转出的 JPEG | 系统临时目录 | 转完立即删除 |
| Messages 自己保存的聊天记录与图片 | `~/Library/Messages/` | 在 Messages → 设置 → 通用 →「保留信息」选一个期限，系统到期自动删 |

## 限制

- 私聊文字、收图；发文字、图片（表情包、图库、生成的图）和语音（TTS 开着时她会用 `send_voice_message`）。群聊忽略。
- 收到的语音和文件只告诉她「有一条语音 / 一个文件」，不转写、不下载。
- 对方的点按回应（❤️👍 等）和「回复某条消息」会作为下一轮的上下文交给她，点按回应本身不触发回复。
- 她没法给对方点按回应：「信息」的 AppleScript 只提供发送，没有回应接口；模拟点击会把窗口抢到前台、系统升级易失效，注入私有框架要关 SIP，都不采用。
- AppleScript 不回报送达，连接器发完回查 chat.db 里的发送/送达/传输状态写进日志，失败只记日志。
- 对方没开 iMessage 时发不出去，不会降级为短信。
- 主动消息（提醒、后台任务完成通知送到手机）还没接上，在 P3。
- 没有 QQ 平台的限流、访客权限和 real_context 等插件，适合只与少数信任的人聊天。
