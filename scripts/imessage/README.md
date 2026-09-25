# iMessage 桥接

让 顾清影 在 iMessage 上和白名单联系人私聊。独立脚本，不改 gqy 本体。

```
LaunchAgent → imessage_bridge.py
  ├ 读 ~/Library/Messages/chat.db，只取白名单联系人的私聊新消息
  ├ gqy ask --session imessage-<联系人名> --create --output-format json …
  ├ 去掉 Markdown，按空行拆成多条
  └ osascript → Messages 发回去
```

## 手机上的快捷指令

在聊天里直接发送，桥接当场回复，不经过模型，也不记进会话：

| 指令 | 作用 |
|---|---|
| `/new` | 开一个新话题（新会话，旧话题都保留） |
| `/topics` | 列出所有话题：轮数、最近一次时间、最后一句的开头 |
| `/topic 2` | 切换到第 2 个话题 |
| `/model` | 看当前模型和可选模型（按供应商分组、带序号） |
| `/model 5` 或 `/model 名字` | 切换模型，只对这个聊天生效；`/model default` 恢复默认 |
| `/pause` / `/resume` | 暂停 / 恢复回复。暂停期间的消息直接跳过，恢复后不会补回 |
| `/help` | 显示指令说明 |

话题 1 就是原来的 `imessage-<联系人名>` 会话，之后的话题是 `imessage-<联系人名>-2`、`-3`……当前话题、模型、暂停状态存在 `~/.gqy/state/imessage-contacts.json`。

## 前提

- 本用户的 Messages 已登录 顾清影 的 Apple ID（Messages → 设置 → iMessage，可以和系统 iCloud 账号不同）。
- 该 Apple ID 已关闭「联系人密钥验证」，否则 iCloud 与 iMessage 账号不一致时无法登录。
- gqy daemon 在运行。

## 安装

```bash
scripts/imessage/install.sh
```

首次安装会生成 `~/.gqy/config/imessage.json`（默认 `enabled: false`）。填好 `contacts`，把 `enabled` 改成 `true`，保存即生效，不用重启。

### 授权

1. **完全磁盘访问**：系统设置 → 隐私与安全性 → 完全磁盘访问权限 → `+`，加入 `~/.local/bin/gqy-imessage`（`install.sh` 编译出的专用启动器），脚本约一分钟内自动重试生效。
   没授权时日志会打 `cannot read … Grant Full Disk Access to …`。
   为什么不直接授权 Python：Xcode 自带的 Python 被系统算作 Xcode 的一部分，授权会落到整个 Xcode 上。启动器只能拉起本脚本（解释器与脚本路径编译时写死），权限只属于它。重复安装内容不变时不会替换启动器，授权不失效。
2. **自动化**：第一次回复时系统会弹「gqy-imessage 想要控制 信息」，点允许。

## 配置 `~/.gqy/config/imessage.json`

| 键 | 说明 | 默认 |
|---|---|---|
| `enabled` | 总开关。关闭期间到达的消息直接跳过，重新打开不会补回 | `false` |
| `contacts` | `[{name, handles}]`。`name` 决定会话名 `imessage-<name>`；`handles` 是手机号或邮箱，同一人的多个 handle 共用一个会话。11 位国内手机号自动补 `+86` | `[]` |
| `gqy_bin` | gqy 可执行文件 | `~/.cargo/bin/gqy` |
| `tools` | 回合可用工具白名单。回合以主人身份运行，别加 `run_command` 这类 | 搜索、看图、记忆、知识库、表情包、图库、生图等 |
| `poll_seconds` | 检查 chat.db 变化的间隔 | `2` |
| `batch_wait_seconds` | 连发时等对方说完再回，每来一条重新计时 | `3` |
| `timeout_seconds` | 单回合超时 | `300` |
| `max_backlog_minutes` | 脚本停机后重启，只补处理这么久以内的消息 | `30` |
| `split_paragraphs` / `max_bubbles` | 按空行拆成多条发送及条数上限。段落超过上限时，相邻段落按长度均衡地合并，不会全挤进最后一条 | `true` / `6` |
| `bubble_pause_seconds` | 连发多条时，每条之前停顿的上限（秒）。按那条的长度停 0.5 秒到这个上限，像在打字；`0` 表示连着发 | `2` |
| `max_memes` | 每轮最多发几张图。她调用 `use_meme`（表情包）或 `album` 的 show（图库）后，脚本按 id 到本机库里取图发送；只发库里登记过、文件头确认是图片、不超过 20 MB 的文件 | `2` |

模型侧的渠道提示在 `hint.txt`（经 `--append-system-prompt` 注入，每回合同一段，不破坏缓存）。

## 运维

| 操作 | 命令 |
|---|---|
| 看日志 | `tail -f ~/.gqy/cache/logs/imessage-bridge.log`（自动轮转，总量封顶约 6 MB；意外崩溃的回溯在 `imessage-bridge.crash.log`） |
| 暂停 | 配置里 `enabled: false` |
| 彻底停止并卸载 | `scripts/imessage/install.sh uninstall` |
| 重启 | `scripts/imessage/install.sh restart` |
| 看会话 | `gqy session list` 里的 `imessage-<name>` |

改 `imessage_bridge.py` 或 `imessage_commands.py` 后，脚本在空闲时（没有进行中的回合）自动用新代码重载。改出语法错误时不重载，日志里会报错并继续跑旧代码。

处理进度记在 `~/.gqy/state/imessage-bridge.json`。回合跑完才推进，中途退出重启后会重新处理没回完的消息。

## 空间占用（都不用手动清理）

| 东西 | 在哪 | 怎么清 |
|---|---|---|
| 桥接日志 | `~/.gqy/cache/logs/imessage-bridge.log*` | 自动轮转，封顶约 6 MB |
| 发表情包前的暂存副本 | `~/Library/Messages/.gqy-send-staging/`（Messages 沙盒只能读这附近） | 每次发送时自动删掉一小时前的 |
| 收到的 HEIC 转出的 JPEG | 系统临时目录 | 回合结束立即删除 |
| 交给 gqy 的图片 | `~/.gqy/state/attachments/` | 归 gqy 会话管理，删会话时一起清 |
| Messages 自己保存的聊天记录与图片 | `~/Library/Messages/` | 在 Messages → 设置 → 通用 →「保留信息」选一个期限，系统到期自动删 |

## 限制

- 支持私聊文字、收图，发表情包、图库图片和她新生成的图片（只发生图插件输出目录里的文件）。群聊忽略。
- 对方的点按回应（❤️👍 等）和「回复某条消息」会作为下一轮的上下文交给她，点按回应本身不触发回复。
- 她没法给对方点按回应：「信息」的 AppleScript 只提供发送，没有回应接口；模拟点击会把窗口抢到前台、系统升级易失效，注入私有框架要关 SIP，都不采用。
- AppleScript 不回报送达，脚本发完回查 chat.db 里每条的发送/送达/传输状态写进日志，失败只记日志。
- macOS 15+ 的 Messages 沙盒只允许发送少数目录里的文件，附件先复制到 `~/Library/Messages/.gqy-send-staging/` 再发。
- 对方没开 iMessage 时发不出去，不会降级为短信。
- 没有 QQ 平台的限流、访客权限和 real_context 等插件，适合只与少数信任的人聊天。
