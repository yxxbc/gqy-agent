# 通用连接器协议：平台接入不再写进 daemon（方案稿）

> 状态：**已确认（09-26）；P2a 已完成（协议、接入端、投递与跟进提到 common/），P2b 施工中**｜日期：2026-09-26｜前身：`docs/design/2026-09-26-imessage-platform.md`（P2 原计划 iMessage 专用模块）

## 一、为什么改

原 P2 计划是给 iMessage 写专用的 `platforms/imessage/` 模块和专用 WS 协议。用户提出：平台接入不该内嵌进 daemon，中间应当有一层协议——以后扩平台方便，两边也不会互相卡住。

改成：**daemon 只写一个通用接入端，定一套带版本号的连接器协议**。平台相关的 I/O（读 chat.db、调 osascript、登 Telegram）全在连接器里，任何语言都能写；daemon 只认协议，不认平台。iMessage 连接器是第一个客户端。QQ 继续走 OneBot（NapCat 本来就是这个结构），不动。

好处：

- 加平台 = 写一个连接器，不改 Rust、不重编 gqy。
- 解耦：连接器自己保管读取水位，daemon 回 ack 才推进。daemon 升级重启或忙，连接器只是等着重连，消息不丢；连接器挂了不影响 daemon。
- macOS 权限只给连接器的启动器，一次授权（原方案稿 §二 的约束不变）。

## 二、协议 `gqy-connector/1`（P2a 已落地，以代码为准：`src/platforms/connector/protocol.rs`）

传输：本机 WebSocket，挂在 web 端口的 `/api/connector/ws?platform=<平台名>`。鉴权在升级之前：该平台必须在 `platforms.connectors` 里启用，并带 `Authorization: Bearer <token>`。**口令必填，不认「来自本机」**：沙盒里的成员会话也能连回环端口（Landlock 不管 socket），放行等于让它冒充主人。帧是 JSON 文本，每帧带 `type`，未知字段忽略。

| 方向 | type | 内容 | 说明 |
|---|---|---|---|
| C→D | `hello` | `protocol`、`platform`、`account`、`display_name`、`connector`（name/version）、`capabilities` | 升级后 10 秒内必须到。platform 要与 URL 一致 |
| D→C | `welcome` | `protocol`、`connection`、`max_frame_bytes`、`max_attachment_bytes` | 版本或平台不对就回 `error`（`bad_hello`）并断开 |
| C→D | `event` | `id`、`kind`（`message`/`reaction`）、`conversation`（kind/id）、`sender`（id/name）、`text`、`reply_to`、`reaction`、`target`、`attachments[]`、`timestamp` | 平台中立字段 |
| D→C | `ack` | `id` | **处理完**才回（回合跑完或判定不回）。连接器收到才推进水位；重复的 id 直接回 ack |
| D→C | `send` | `req`、`to`、`part`（`text`/`image`/`audio`/`file`，附件带 mime/name/data） | 一帧一个气泡或一个附件；拆气泡、打字停顿都在 daemon |
| C→D | `send_result` | `req`、`ok`、`message_id`、`error` | 纯文字 30 秒、附件 180 秒内不回算失败 |
| 双向 | `ping`/`pong` | — | daemon 30 秒一次；90 秒收不到任何帧就断 |
| 双向 | `error` | `code`、`message` | |

附件走协议内 base64（单个上限 16 MiB，单帧 32 MB），不传路径：daemon 没有完全磁盘访问权限，读不了 `~/Library/Messages`；以后远端连接器也一样能用。

`capabilities`：`reaction_in`、`reaction_out`、`image_out`、`audio_out`、`file_out`、`group`，缺省全否。没声明的能力 daemon 不用（没有 `audio_out` 时 `send_voice_message` 报「cannot send voice messages」）。

同一（平台, 账号）只留一条连接：重连顶掉旧连接，旧连接上等待中的发送立刻失败。

## 三、daemon 侧结构

```
src/platforms/
  connector/          通用接入端
    mod.rs            ConnectorDriver（配置变了断开不再合法的连接）
    protocol.rs       帧类型
    server.rs         WS 入口、鉴权、握手、读写循环、心跳、ack
    registry.rs       连接表、事件去重、攒着的点按回应
    inbound.rs        event → 回合（照 QQ 私聊的规矩）
    adapter.rs        PlatformAdapter：拆气泡、打字停顿、附件、按能力拒绝
    policy.rs         impl PlatformPolicy for ConnectorPlatformConfig
    commands.rs       （P2b）/new /topics /topic /model /pause /resume /help
    tests.rs          假连接器走真 WebSocket
  common/delivery.rs  deliver_dispatch（从 onebot/outbound.rs 提出来，两边共用）
  common/followup.rs  回合中途来消息：platform_update_target、active_turn_update_mode
```

- **配置**：`platforms.connectors.<platform>`（缺省不写出），每个平台一段：`enabled`、`token`、`contacts`（名字 + 多个账号合成一个人，`owner` 标主人）、`owner_host_tools`、`max_bubbles`、`bubble_pause_seconds`、`memory_write_enabled`。token 在 WebUI 里打码（照 `platforms.qq.access_token`）。旧桥接的「攒几秒再回」不要了：私聊里回合还在写时来新消息会取代它，和 QQ 私聊一样。
- **会话**：绑定键 `platform=imessage, conversation_id=<联系人名>`。话题 = 同一联系人名下多个会话，`/new` 建新会话并改绑，`/topic N` 改绑到 `imessage-<联系人>-N`。旧会话 `imessage-<联系人>`、`imessage-<联系人>-N` 原名沿用，历史不丢；当前话题从 `~/.gqy/state/imessage-contacts.json` 迁过来一次。
- **模型**：`/model` 用会话级模型覆盖（`set_session_model_override`，`TurnProfile.text_models = None` 时自动生效），不写配置文件。
- **权限**：联系人名就是身份（`sender_id`），同一个人的多个账号合成一个。`owner: true` 的联系人按主人算（记忆共享、写入算主人的），但**宿主工具默认关**（`owner_host_tools`）：手机丢了或账号被盗时，别人不能借聊天窗口在电脑上跑命令。关着时用受限工具底座，再把记忆工具换成主人作用域。
- **插件**：第一版不开平台插件（插件缺省只服务 QQ，不少还写死了 onebot），按需再逐个放开。
- **语音**：不另做 `<voice>` 标签。平台回合本来就有 `send_voice_message` 工具（TTS 可用时注册，合成后以 `AudioPath` 发出、用完即删），连接器声明 `audio_out` 就能收到 `audio` part。旧桥接 WIP 里的标签方案作废。
- **唤醒**：`job_wake.rs` 先按 `binding.key.platform` 分派，连接器平台的会话暂不唤醒（不再被当成 QQ 去唤醒）；真正的主动消息在 P3。

## 四、iMessage 连接器（瘦身）

`scripts/imessage/imessage_bridge.py` 原地改写（文件名和路径不能变：启动器把路径编进二进制，一改就要重新授权，AGENTS.md §4.5）。只留：

- 读 chat.db（查询、attributedBody 解码、HEIC→JPEG、引用、点按回应）→ `event` 帧；收到 `ack` 才推进水位。
- 收 `send` 帧 → osascript 发出 → `send_result`；回查送达状态。
- 断线重连（指数退避）、脚本热重载、日志。

删掉：`gqy ask` 调用、指令、会话命名、气泡拆分、表情包/图库/生图解析、语音合成——全搬进 daemon。`imessage_commands.py`、`hint.txt` 删除（提示词进 daemon 的平台上下文，英文）。实际约 1000 行，比预估多，多在标准库手写的 WebSocket 客户端（启动器用系统 Python，不引入 pip 依赖）。

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
