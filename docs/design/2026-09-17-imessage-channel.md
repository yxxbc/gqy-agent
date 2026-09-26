# iMessage 双向通道：实现方案

> 状态：**已施工并迭代两轮；本文所述的桥接内回合逻辑已被 09-26 的平台化取代**。09-26 复核：iMessage 现在是 gqy 的原生平台（`platforms.connectors.imessage`），`scripts/imessage/` 只剩收发 I/O，会话、指令、拆气泡、模型、记忆都由 daemon 处理——见 `docs/design/2026-09-26-imessage-platform.md`（平台层整理）与 `docs/design/2026-09-26-connector-protocol.md`（通用连接器协议，P2a/P2b/P2c 已合入 gqy）。本文保留为当时的设计与取舍记录｜日期：2026-09-17｜实现：`scripts/imessage/`
>
> 文中 `文件:行号` 均指 `9a82439f` 时的代码。

---

## 0. 一句话

顾清影 的 Apple ID 直接登录在 `mac` 用户的 Messages 里。独立 Python 脚本读 `chat.db` 收白名单联系人的私聊，交给 `gqy ask` 跑回合，再用 AppleScript 发回。gqy 本体不改，不开 Private API，不关 SIP。

---

## 1. 演进与否决项

| 方案 | 结论 | 原因 |
|---|---|---|
| 独立 macOS 用户 `gqy` + 20888 发送服务（原状） | 否决 | 需常驻第二个登录会话（实测 423 个进程）。跨用户读库、更新都要 sudo |
| 自研 Rust 桥接反向连 daemon | 否决 | 仍需第二个用户会话 |
| BlueBubbles | 否决 | Electron 常驻 200–400 MB。Releases 最新只提到 Sequoia，macOS 27 兼容未知 |
| Private API | 否决 | 需关整机 SIP，版本敏感 |
| 原生平台适配器 `platforms/imessage` | 暂不做 | 核心层有 101 处直读 `config.platforms.qq`，接入会波及 QQ。单人使用收益不抵 |
| **Messages 登录独立 Apple ID + 脚本 + `gqy ask`** | **采用** | 零核心改动，会话/记忆/人格现成 |

关键事实：

- Messages 的 iMessage 账号可以与系统 iCloud 账号不同。前提是该 Apple ID 关闭「联系人密钥验证」（其要求 iCloud 与 iMessage 同账号）。
- 切换后本机不再收发用户本人的 iMessage，iPhone 不受影响。

---

## 2. 设计要点

### 2.1 会话

- 一个联系人一个会话：`gqy ask --session imessage-<name> --create`。
- 联系人按 `name` 聚合多个 handle（手机号、邮箱），避免同一人被拆成多个会话。不用 `chat_guid`，它的前缀会在 `iMessage;-;` / `SMS;-;` / `any;-;` 间变化。
- 与 QQ 会话互相独立，不做跨平台身份合并。
- 回合以主人身份运行，记忆照写。

### 2.2 权限

- 白名单外的消息直接跳过，不进任何历史。
- `--tools` 限定为搜索、看图、记忆、知识库等，不含 `run_command`，防号码冒用。
- 完全磁盘访问授给系统 Python，这是已知妥协（该 Python 跑的所有脚本都获得读盘权限）。后续可打包成独立可执行文件再收窄。

### 2.3 收

- 按 `chat.db` 与 `-wal` 的 (mtime, size) 判断有无变化，变了才查。
- 跳过己方消息、点按回应（`associated_message_type`）、群事件（`item_type`）、群聊（`chat.style = 43`）。
- `text` 为空时解码 `attributedBody`。
- 附件等文件下载完成。HEIC 用 `sips` 转 JPEG 后经 `--image` 交给模型。

### 2.4 回合与发送

- 同一联系人串行。回合进行中到达的消息攒到下一轮。连发时按 `batch_wait_seconds` 静默期合并。
- 渠道提示 `hint.txt` 经 `--append-system-prompt` 注入。每回合同一段，前缀稳定（`docs/cli-backend.md` §5）。按 AGENTS §1.5 用英文短句。
- 回复先走与 `src/platforms/reply.rs:396` 同口径的 `markdown_to_plain`（另去 `~~`），再按空行拆成多条。
- AppleScript 正文走 argv，不拼源码。发送后回查 `chat.db` 己方新消息的 `error` 字段。

### 2.5 可靠性

- 水位分 seen 与 durable，回合跑完才推进 durable 并落盘。停机超过 `max_backlog_minutes` 的旧消息不回灌。
- 配置按 mtime 热加载。`enabled: false` 期间的消息跳过且不补。
- 脚本改动后在空闲时 `execv` 自重载，编译不过则保留旧代码。
- `gqy ask` 报 busy 时重试两次。

---

## 3. 未做 / 后续

- 出站图片（`stream-json` 的 `image` 事件 → 取 asset → `send POSIX file`）。
- 常驻 `gqy stdio` 替代逐条起进程。
- 打包独立可执行文件，收窄完全磁盘访问权限。
- 下线 `/Users/Shared/gqy_bridge/` 与 `gqy` 用户里的 LaunchAgent。
