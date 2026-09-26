# 通用连接器协议：平台接入不再写进 daemon（方案稿）

> 状态：**已确认（09-26），P2a 施工中**｜日期：2026-09-26｜前身：`docs/design/2026-09-26-imessage-platform.md`（P2 原计划 iMessage 专用模块）

## 一、为什么改

原 P2 计划是给 iMessage 写专用的 `platforms/imessage/` 模块和专用 WS 协议。用户提出：平台接入不该内嵌进 daemon，中间应当有一层协议——以后扩平台方便，两边也不会互相卡住。

改成：**daemon 只写一个通用接入端，定一套带版本号的连接器协议**。平台相关的 I/O（读 chat.db、调 osascript、登 Telegram）全在连接器里，任何语言都能写；daemon 只认协议，不认平台。iMessage 连接器是第一个客户端。QQ 继续走 OneBot（NapCat 本来就是这个结构），不动。

好处：

- 加平台 = 写一个连接器，不改 Rust、不重编 gqy。
- 解耦：连接器自己保管读取水位，daemon 回 ack 才推进。daemon 升级重启或忙，连接器只是等着重连，消息不丢；连接器挂了不影响 daemon。
- macOS 权限只给连接器的启动器，一次授权（原方案稿 §二 的约束不变）。

## 二、协议 `gqy-connector/1`

传输：本机 WebSocket，挂在 web 端口的 `/api/connector/ws`。鉴权：`Authorization: Bearer <token>`，token 在该连接器的配置里；没配 token 时只接受回环地址。帧是 JSON 文本，每帧带 `type`。

| 方向 | type | 内容 | 说明 |
|---|---|---|---|
| C→D | `hello` | `protocol`、`platform`（如 `imessage`）、`account`、`connector`（名称/版本）、`capabilities` | 第一帧。platform 必须在配置里启用 |
| D→C | `welcome` | `protocol`、`session`（连接 id）、`limits`（单帧/附件上限） | 版本不兼容就回 `error` 并断开 |
| C→D | `event` | `id`（连接器侧唯一，如 ROWID）、`kind`（`message`/`reaction`）、`conversation`、`sender`、`text`、`reply_to`、`reaction`、`attachments[]`、`timestamp` | 平台中立字段，对应 `PlatformInboundEvent` |
| D→C | `ack` | `id` | daemon 已接收并排进队列。连接器收到才推进水位 |
| D→C | `send` | `req`、`conversation`、`parts[]`（`text`/`image`/`audio`/`file`/`reaction`）、`pace_ms` | 一个气泡或一个附件一帧；daemon 负责拆气泡和节奏 |
| C→D | `send_result` | `req`、`ok`、`message_id`、`error` | 送达回执（iMessage 回查 chat.db 后可补一帧 `delivery`） |
| 双向 | `ping`/`pong` | — | 30 秒心跳 |
| 双向 | `error` | `code`、`message` | |

附件走协议内 base64（带大小上限），不传路径：daemon 没有完全磁盘访问权限，读不了 `~/Library/Messages`；以后远端连接器也一样能用。

`capabilities` 例：`{"reaction_in": true, "reaction_out": false, "audio_out": true, "group": false, "typing": false}`。daemon 按能力决定能用什么（例如没有 `audio_out` 就不合成语音）。

## 三、daemon 侧结构

```
src/platforms/
  connector/          通用接入端（新）
    mod.rs            ConnectorDriver（PlatformDriver 实现）、连接表
    protocol.rs       帧类型 serde，版本协商
    server.rs         WS 握手、鉴权、读写循环、ack
    inbound.rs        event → PlatformInboundEvent → 回合（照 onebot/dispatch 的 14 步）
    adapter.rs        ConnectorAdapter：PlatformAdapter 实现，send → send 帧 + 等 send_result
    commands.rs       连接器平台通用指令：/new /topics /topic /model /pause /resume /help
    policy.rs         impl PlatformPolicy for ConnectorPlatformConfig
  common/delivery.rs  deliver_dispatch 从 onebot/outbound.rs 提出来，两边共用
```

- **配置**：`platforms.connectors.<platform>`（缺省不写出），每个平台一段：`enabled`、`token`、`owner`、`contacts`（名字 + 多个 handle 合成一个人）、`batch_wait_seconds`、`max_bubbles`、`tools`、`voice`。token 在 WebUI 里打码（照 `platforms.qq.access_token`）。
- **会话**：绑定键 `platform=imessage, conversation_id=<联系人名>`。话题 = 同一联系人名下多个会话，`/new` 建新会话并改绑，`/topic N` 改绑到 `imessage-<联系人>-N`。旧会话 `imessage-<联系人>`、`imessage-<联系人>-N` 原名沿用，历史不丢；当前话题从 `~/.gqy/state/imessage-contacts.json` 迁过来一次。
- **模型**：`/model` 用会话级模型覆盖（`set_session_model_override`，`TurnProfile.text_models = None` 时自动生效），不写配置文件。
- **插件**：第一版只开平台中立的 reply_processor；其余插件写死了 onebot，按需再放开。
- **语音**：`<voice>` 标签在 daemon 解析，复用 `ui.tts` 配置与语音模块合成，作为 `audio` part 发给连接器。缓存按天清理。
- **唤醒**：`job_wake.rs` 里写死 onebot 的地方改成按 `binding.key.platform` 分派——这是 P3 主动消息的前提，P2 顺手把分派口留好。

## 四、iMessage 连接器（瘦身）

`scripts/imessage/imessage_bridge.py` 原地改写（文件名和路径不能变：启动器把路径编进二进制，一改就要重新授权，AGENTS.md §4.5）。只留：

- 读 chat.db（查询、attributedBody 解码、HEIC→JPEG、引用、点按回应）→ `event` 帧；收到 `ack` 才推进水位。
- 收 `send` 帧 → osascript 发出 → `send_result`；回查送达状态。
- 断线重连（指数退避）、脚本热重载、日志。

删掉：`gqy ask` 调用、指令、会话命名、气泡拆分、表情包/图库/生图解析、语音合成——全搬进 daemon。预计从 1466 行降到 500 行左右。`imessage_commands.py`、`hint.txt` 删除（提示词进 daemon 的平台上下文，英文）。

## 五、分步与验收

| 步 | 内容 | 验收 |
|---|---|---|
| P2a | 协议 + 通用接入端 + `common/delivery` + 配置 + 协议文档；用假连接器（测试里的 WS 客户端）跑通收发 | 测试：握手、鉴权、ack、回合、send/send_result、断线 |
| P2b | iMessage 连接器改写 + 指令 + 会话迁移 + 语音 | 手机上把现有功能各试一遍；旧会话历史还在 |
| P2c | WebUI 平台页（连接器列表、状态、联系人、设置）+ TUI 平台菜单一项 | 页面上能看到连接状态、改设置 |

每步单独提交；P2 整体验收通过再合进 gqy、写 CHANGELOG。

## 六、用户拍板（09-26，均按推荐）

1. **通用连接器协议**，不写 iMessage 专用模块；QQ 继续走 OneBot。
2. **配置放 `platforms.connectors.<platform>`**，取代原方案稿的 `platforms.imessage`。
3. **语音 WIP 先原样提交**（`bce1e5de`）再改写，合成搬进 daemon。
4. **直接切换**：连接器原地替换旧脚本，出问题用 git 退回，不留新旧双模式。
