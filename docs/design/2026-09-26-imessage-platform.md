# iMessage 变成原生平台，平台层整理成可扩展结构（方案稿）

> 状态：**P1 已完成（09-26，`f3f3da60` 合入 gqy）；P2 起改走通用连接器协议，P2a / P2b / P2c 已完成并合入 gqy（`4b909420`），见 `docs/design/2026-09-26-connector-protocol.md`；剩 P3**（本文 §三 的 `imessage/` 模块与 §三.1 的 `platforms.imessage` 作废）｜日期：2026-09-26｜前身：`docs/design/2026-09-17-imessage-channel.md`（当时判「原生平台适配器暂不做」）

## 一、为什么现在做、要解决什么

iMessage 现在是 `scripts/imessage/` 下的独立 Python 桥接：自己读 `chat.db`、自己拼会话名、自己调 `gqy ask`、自己处理指令和气泡。QQ 有的东西它一样都没有：WebUI 平台页、会话级模型路由、记忆作用域、主动消息（提醒送不到手机）、定时消息、主动私聊、睡眠时段、消息记录面板。每加一个功能都要在 Python 里重写一遍。

09-17 判「暂不做」的理由是平台层只为 QQ 写（核心层 101 处直读 `config.platforms.qq`）。09-26 摸底（全文见本次会话的耦合图）结论比当时乐观：

- **数据类型和回合引擎已经是平台中立的**：`platform_types.rs` 有 `PlatformConversation`、`PlatformPrincipal`、`OutboundMessage` 和 `trait PlatformAdapter`（只有 `send` 与 `bot_display_name` 必须实现，其余默认「不支持」）；`scheduling.rs`（会话绑定、限流）、`reply.rs`、`live_turns.rs`、`inflight.rs`、`turn_order.rs`、`activity.rs`、`commands.rs` 都中立；§4.3 两道投递幂等闸（图片 digest、文字 bigram）就在中立的 `turn_context.rs` 里。
- **QQ 耦合集中在五处**：配置形状（`platforms.qq`）、连接生命周期（`PlatformRuntime` 写死 `onebot` 与 `qq_listener` 两个字段）、唤醒与主动发送入口（`onebot/proactive.rs`、`wake_conversation_for_job`）、插件启用（全读 `platforms.qq.plugins`）、约 15 处写死的 `"onebot"` 字符串。

所以这次分两件事：先把平台层整理成「挂多个平台」的结构（**不改 QQ 的任何行为**），再把 iMessage 作为第二个平台接进来。以后 Telegram、QQ 官方机器人照同一条路走。

## 二、关键约束：macOS 权限只能给一个不变的小程序

读 `chat.db` 要「完全磁盘访问权限」，发消息要「自动化 → 控制信息」。macOS 把这两项授给**具体的二进制**：

- `gqy` 是 ad-hoc 签名，每编译一次签名标识就变（实测 `Identifier=gqy-1257bc8fa4792422`）；Nix 下路径每次升级都变（AGENTS.md §7.4）。daemon 自己读 `chat.db` 的话，**每次升级都要重新授权**。
- 权限按「负责进程」判定：daemon 拉起的子进程算 daemon 的，一样拿不到。

现在的桥接正是为此单独编了一个启动器（`launcher.c`，解释器与脚本路径编译时写死，内容不变就不替换，授权不失效），由 LaunchAgent 拉起。**这个结构要保留**：

```
LaunchAgent → gqy-imessage 启动器（持有两项权限，一次授权）
                └ 连接器：只做 I/O —— 读 chat.db 新消息 → 推给 daemon；
                          收 daemon 的发送请求 → osascript 发出 → 回执
                      ⇅  本机 WebSocket（带 token，仿 OneBot 反向 WS）
gqy daemon → 平台 imessage：会话、指令、插件、模型路由、记忆、气泡、幂等闸、主动消息……
```

连接器之于 iMessage，就像 NapCat 之于 QQ。它的代码越少越稳定：逻辑改动都落在 daemon，连接器很少需要改，改了也只是重载脚本，不动启动器、不丢授权。`scripts/imessage/` 路径不动（AGENTS.md §4.5）。

## 三、平台层的新结构

```
src/platforms/
  mod.rs          PlatformRuntime{ drivers: Vec<Arc<dyn PlatformDriver>>, … }
  driver.rs       trait PlatformDriver：id / prepare→commit（配置重载两段式）/ shutdown /
                  status / connected / send_direct（主动发送）/ wake（任务唤醒）
  policy.rs       PlatformPolicy：一次回合用到的平台策略（主人/管理员/白名单、宿主工具、
                  记忆写入、中间消息、会话限额、模型路由与模型池、最大回复长度、插件实例）
  common/         平台中立的部分从现在的顶层搬进来：turn_context、turn_run、scheduling、
                  reply、delivery（从 onebot/outbound.rs 提出来的 deliver_dispatch）、activity、
                  inflight、live_turns、turn_order、commands、access_control、logging
  plugins/        插件描述符新增 platforms 字段，注册表按平台过滤
  onebot/         QQ，基本不动；新增 driver.rs 包住 QqListenerManager 与 proactive/wake
  imessage/       #[cfg(target_os = "macos")]：driver.rs、connector.rs（WS 协议）、
                  inbound.rs、adapter.rs、contacts.rs、commands.rs（/new /topics /model …）
```

要点：

1. **配置**：`platforms.qq` 一个字节都不动（它牵着 100 多处测试和用户现有配置）；新增 `platforms.imessage`（缺省不写出）。QQ 下那些其实通用的字段（模型池、会话限额、记忆、睡眠时段、会话路由……）不搬家，而是由 `PlatformPolicy` 按平台取：QQ 从 `platforms.qq` 取，iMessage 从 `platforms.imessage` 取。以后要统一成 `platforms.<id>` 时只改 policy 的实现。
2. **生命周期**：`server.rs` 启停、`ipc_server.rs` 与 `config_api.rs` 的重载两段式都改成遍历 drivers。QQ 的 `QqListenerManager` 包一层，不重写。
3. **写死的 `"onebot"`**（job_wake、session_ops、goal_driver、pop_cmds、usage_query 等）改成按会话绑定里的 `platform` 找对应 driver。
4. **插件**：描述符加 `platforms`，默认只给 onebot，现有插件行为不变；iMessage 第一版只开 reply_processor（和私聊部分的 message_history）。
5. **搬文件**按 `docs/fixed/2026-08-18-代码拆分.md` §五 的五个坑来（include_str 相对路径、模块名遮蔽、super 语义、脚本拒绝覆盖、回退先看暂存区），只搬不改，搬完门禁与全部测试过了再动逻辑。

## 四、iMessage 平台能做什么

**第一版（和现在的桥接功能对齐，但全在 daemon 里）**
- 白名单联系人私聊，一个联系人多个 handle 合成一个人；收文字、收图（HEIC 转 JPEG）、引用回复与点按回应作为上下文。
- 快捷指令 `/new` `/topics` `/topic` `/model` `/pause` `/resume` `/help`，改成平台指令（和 QQ 的 `command_prefix` 指令同一套机制）。
- 发文字（均衡拆气泡、打字节奏）、发表情包 / 图库 / 生成的图；§4.3 两道幂等闸自动生效。
- 会话：沿用现有 `imessage-<联系人>`、`imessage-<联系人>-N` 会话，**历史不丢**（迁移时把这些会话绑定到对应联系人）。

**第二版（平台化之后白拿或小改就有）**
- **主动消息送达手机**：她说「明天 8 点提醒你」、闹钟和后台任务完成通知，能发到 iMessage（`send_direct` + `wake`）。这就是之前说要单独写方案的那一条。
- 定时消息、主动私聊（她自己决定什么时候找你）、睡眠时段。
- WebUI 平台页多一个 iMessage：连接状态、联系人、设置、会话级模型路由、消息记录。
- 送达回执：连接器回查 `chat.db` 的发送状态，失败时告诉她（现在只写日志）。

**不做**：群聊（和 QQ 群一样要限流、访客权限、触发规则，另立项）；她给你点按回应（「信息」没有接口，见 scripts/imessage/README.md）。

## 五、分期与验收

| 期 | 内容 | 行为变化 | 验收 |
|---|---|---|---|
| P1 ✅ | 平台层整理：common/ 搬家、PlatformDriver 包住 QQ、PlatformPolicy、插件按平台过滤、去掉写死的 onebot | 无 | 全部测试与门禁绿；QQ 私聊、群聊、定时消息、主动私聊各走一遍 |
| P2 | iMessage 平台第一版 + 连接器瘦身 + 会话迁移；WebUI 平台页、TUI 平台菜单各加一项 | 桥接换成平台 | 手机上把现有功能各试一遍；旧会话历史还在 |
| P3 | 主动消息、定时消息、主动私聊、睡眠时段、送达回执、消息记录 | 新功能 | 让她定一个提醒，到点手机收到 |

每期单独分支、单独验收、单独提交。P2 上线后旧桥接的回合逻辑删掉，只留连接器。

注（09-26）：另有一路改动在 `scripts/imessage/imessage_bridge.py` 里加语音消息（用户明确要语音时，回复里的 `<voice>…</voice>` 用 MiniMax 等 TTS 合成成音频发出），写方案时尚未提交。P2 迁移时一并搬进平台：合成放 daemon（复用 `ui.tts` 配置与语音模块），连接器只负责把音频文件发出去。

## 六、P1 实际落地（09-26）

- 目录叫 `common/` 不叫 `core/`：`core` 会遮住标准库的 `core`（搬文件五坑之「模块名遮蔽」）。`platforms/mod.rs` 原名再导出，旧路径不用改。
- `PlatformDriver` 只落了 `id / display_name / prepare→commit / shutdown`；`status / connected / send_direct / wake` 留给 P2、P3 按需加。
- `PlatformPolicy` 落了 `plugin_enabled / is_owner / private_whitelisted / allow_non_admin_host_tools / intermediate_messages`；模型路由、会话限额等仍直读 `platforms.qq`，P2 接 iMessage 时再按需收进 policy。
- `deliver_dispatch` 还在 `onebot/outbound.rs`，没提成 `common/delivery`，P2 需要时再提。
- 写死的 `"onebot"` 改成了遍历 `PLATFORM_IDS` 的 `*_all_platforms` 查询。

## 七、用户拍板（09-26，均按推荐）

1. **连接器 + 原生平台**：不让 daemon 直接读 `chat.db`，否则每次升级都要重新授权。
2. **新增 `platforms.imessage`，`platforms.qq` 不动**：不统一成 `platforms.<id>`。
3. **P1 就做目录整理**（common/ 搬家），不只加接缝。
4. **第一版只对齐现有功能**，主动消息放 P3。
