# 多设备同步（多主，无常开机器）方案稿

> 状态：**方案稿，待用户拍板（§八 的决策点）**｜日期：2026-09-27
>
> 来由：用户对照 Operit2（`AAswordman/Operit2`）后想借鉴它的设备同步。已定：**多主同步**
> （每台都跑完整 daemon，离线可用，联网后合并），**没有常开机器**（笔记本为主，多台都可能是主力）。

## 一、Operit2 是怎么做的，哪些能抄

调研对象：Operit2 `d72f791`（2026-09-27），`core/crates` 下的 persistence / node / access。

| 机制 | Operit2 做法 | 结论 |
|---|---|---|
| 存储归属声明 | `RuntimeStorageLayout` 给每个路径声明 `Space`（同步）/`CoreNode`（本机）/`Ephemeral`，未声明即报错 | **抄**。gqy 已有 `transfer/registry.rs` 的分级清单，扩成同一张表 |
| 操作日志 | 每源设备一个序号 + 向量时钟；操作是「实体快照」，分 `EntityState`（可压缩，只留最新）和 `Transaction`（全留） | **抄思路** |
| 捕获方式 | 仓储写完库后**另起事务**回读再记日志 | **不抄**：写库与记日志不原子，崩溃即漏。改成同事务 outbox |
| 冲突 | 非聊天域按**墙钟** LWW；聊天域只比同源，**并发写会分叉不收敛** | **不抄**。改 HLC 全序 + 会话单写者 |
| 大文件 | SHA-256 内容寻址 blob，操作只带 hash | **抄** |
| 任务接续 | Binding `{key,nodeId,generation}` + `switch_core` 工具 | 概念可借；但它的 generation 防重执行在代码里没落地，分区时两端可同跑同一会话 |
| 传输 | 设备间 HTTP 长轮询/WS，mDNS 发现，**无中继/NAT 穿透**，明文 http | **不适用**：无常开机器时两台设备常常不同时在线 |
| 收尾 | 日志与 blob 永不 GC；入网只单向拉取 | 自己补 |

一句话：Operit2 的**数据模型骨架**值得借，**冲突、原子性、传输**都要重新设计（它自己这块约 1.8 万行且未收敛）。

## 二、gqy 现状（09-27 盘点，均为仓库相对路径）

**没有任何设备身份**。所有归属判断靠本机 PID / 文件锁 / 进程内标志：

- 回合认领：`turns.status='running' + owner_pid`；`recover_stale_running_turns`
  （`src/state/conversation_db/turns.rs:563`）用本机 `kill(pid,0)`（`src/alarm.rs:231`）判活——
  **同步进来的别机 running 回合会被本机当死回合改成 interrupted**。排队提示词同理
  （`queue/consume.rs:415-458`）。
- 单例：daemon 用 flock（`src/ipc/lifecycle.rs`）；运行闩在内存（`ManagerState.active_runs`）。

**数据分布与合并难度**（难度从高到低）：

1. **记忆库** `personas/<id>/memory/memory.db`：facts / episodes / pending_events / memory_revisions
   全是 `AUTOINCREMENT`（`src/memory/schema.rs:14` 起），且被 JSON 引用（`source_episode_ids`、
   `source_key`、`memory_revisions.memory_id`、`memory_embeddings(kind,id)`）；重置会清
   `sqlite_sequence`（`src/memory/write.rs:18,208`）；**每次联想召回都原地改** `recall_count/strength`
   （`recall.rs:605,617`），开库衰减也改；整理器用 `memory_meta.database_id/generation` 做身份闸。
2. **会话库** `conversation.db`（schema v39）：`turns.seq` 每会话 MAX+1（`conversation_db/mod.rs:289`），
   完成时再挪到 MAX+1（`mod.rs:568`）；compact / 撤销按 seq 区间；redo 删了再恢复子表；
   多张子表按 rowid 排序（`turn_tool_reports`、`turn_journal_events`、`turn_inline_media`）。
   **好消息**：session_id / turn_id 是文本 ID，完成的回合内容是化石化、只追加的。
3. **平台消息历史** `history.sqlite3`：`context_boundaries.after_row_id` 指向本地 rowid；两台都连同一 QQ 号会重复入库。
4. **唯一名索引**：账号用户名、账本的账本/账户/分类名——每台各自播种默认值，名字同、ID 不同。
5. **全局计数**：`state/usage.json` 累计值、会话 token 合计、`sessions.sort_key`、`app_state` 当前会话指针。

账本 `ledger.db` 主键全是文本随机 ID + `revision` + 软删除，**最好合并**。表情库 ID 是内容 hash，也好合并。

**只能在一处发生的事**：

| 事项 | 现状 | 两台都跑的后果 |
|---|---|---|
| QQ 定时消息 | 20 秒 tick，已发集合在内存（`platforms/plugins/scheduled_messages/mod.rs:43`） | 都连着 NapCat 就**重复发** |
| QQ 主动私聊 | 计划+日计数在 `state/qq_private_initiative.json` | **重复私信**、日上限被绕过 |
| QQ / iMessage 连接 | NapCat 反向 WS 连 daemon；连接器连一台 daemon | 配多个地址就**每台都回** |
| 记忆整理器 | 批处理 `consolidated_at IS NULL` 的日记（`write.rs:335`） | 合并后两台都整理同一批日记 → **重复长期记忆** |
| goal 自动轮 | 库内 CAS 认领（`goals.rs:480`），只在同库有效 | 各自认领第 N+1 轮 → 历史分叉 |
| 闹钟 / 后台任务 | 状态文件里存**本机 PID** | 同步过去会误杀别的进程——**必须本机** |

**绑死本机的会话属性**：`sessions.workspace`（沙盒根，绝对路径）、`turns.workspace`（cwd）、
spill 文件的绝对路径**写进了化石化的工具输出**（`src/agent/pruning.rs:25-50`）、中转线续传表
`state/relay/sessions.json` 指向本机 `~/.claude` 等目录里的 CLI 会话。

## 三、总体方案

```
 设备 A（Linux）                中转存储（同步盘文件夹 / WebDAV / S3）            设备 B（Mac）
 ┌───────────────┐   只写自己的目录   ┌─────────────────────────────┐   只写自己的目录   ┌───────────────┐
 │ daemon        │ ─────────────────▶ │ space.json（成员、密钥指纹）  │ ◀───────────────── │ daemon        │
 │  SQLite 各库  │                    │ devices/A/log/000123.seg     │                    │  SQLite 各库  │
 │  sync_outbox  │ ◀── 读别人的日志 ── │ devices/A/cursor.json        │ ── 读别人的日志 ──▶ │  sync_outbox  │
 │  sync_inbox   │                    │ devices/B/log/… cursor.json  │                    │  sync_inbox   │
 └───────────────┘                    │ blobs/<sha256>               │                    └───────────────┘
                                      └─────────────────────────────┘
                                       全部内容端到端加密，存储方只见密文
```

### 3.1 中转：「笨存储 + 每设备只写自己的目录」

无常开机器 → 不能指望两台同时在线 → **不做设备直连，做异步信箱**。中转只要求「能存文件」：

- **v1：本地文件夹后端**。用户指向任何会自动同步的目录：iCloud Drive、坚果云、Dropbox、
  OneDrive、Syncthing（Syncthing 也要两端同时在线，不推荐作唯一中转）。
- 以后：WebDAV（坚果云 WebDAV 免客户端）、S3 兼容（R2 等）后端，同一个 trait。
- **每台设备只写 `devices/<自己>/`**，别人的只读。同步盘层面永远不会出现同一文件被两台同时改，
  「冲突副本」文件从根上不存在。日志按段文件追加，段写满/定时封口后不再改动。
- `cursor.json` 发布「我已应用到各设备的第几号」，用于判断哪些段所有人都读过、可以清理（§3.6）。

**绝不同步 SQLite 文件本身**：同步盘拷活库 = 损坏（AGENTS §3.3，08-21 事故），也无法合并。

### 3.2 设备身份与加密

- 首次启用生成 `device_id`（ULID）与设备名，存 `state/device.json`（本机，不同步）。
- 空间密钥：建空间时生成 32 字节随机密钥；新设备加入时用户在老设备上 `gqy sync invite`
  拿到一串口令（或二维码），新设备 `gqy sync join <口令>`。口令 = 空间密钥 + 中转位置。
- 段文件与 blob 用 XChaCha20-Poly1305 加密（关联数据 = 设备 ID + 段号，防挪用）；存储方只看到密文。
  密钥放系统钥匙串或 0600 文件。
- 设备移除：`space.json` 记录成员，移除后**轮换密钥**（旧设备读不了新段）。

### 3.3 时钟与日志格式

- 每条操作：`{op_id: "<device>:<seq>", hlc, domain, kind, key, payload, schema}`。
- `seq` 每设备单调，保证「缺没缺」可判断；**HLC（混合逻辑时钟）**给全序，用于 LWW，
  不受墙钟回拨影响（Operit2 用墙钟排序是它会跳号的根因之一）。
- 两类语义沿用 Operit2：`State`（只要最新一条，可压缩）与 `Event`（全留，如「新增一个回合」）。
- **捕获 = 同事务 outbox**：业务写库与写 `sync_outbox` 在同一个 SQLite 事务里；后台把 outbox
  打包成段上传。收到的段先落 `sync_inbox`，再按域应用，应用与记账同事务。崩溃任何一刻都不丢不重。
- 捕获点放在**领域函数**（回合完成、会话元数据变更、写记忆、记账），不放在每个 SQL、也不用触发器：
  触发器只能看到行和本地自增 ID，正是要避开的东西。

### 3.4 各类数据怎么合并

**会话与回合 —— 单写者 + 冲突即分叉（核心）**

- 会话多一列 `owner_device` + `owner_epoch`。只有 owner 设备能往里追加回合。
- 同步单位 = **完成的回合**（`Event`）：回合行 + 其子表（工具报告、化石 context_messages、
  内联媒体、问答）打成一个包，图片/附件走 blob。别机的副本按 owner 给的 seq 原样落库——
  单写者下 `seq` 不会撞，前缀字节与 owner 一致，**缓存契约不受影响**。
- 撤销、compact 水位、隐藏、改标题 / 人格 / 模型覆盖 → 会话级 `State`/`Event` 操作，由 owner 发出。
- **换设备接着聊 = 接管**：B 打开 A 拥有的会话并发消息时，写一条 `takeover{epoch+1, base_turn}`，
  随后 B 成为 owner。没有常开机器就没有强租约，所以接管是「乐观」的：
  - A 此后上线看到 epoch 变了，就停止在该会话追加。
  - 若 A 离线期间也在这个会话里聊了（base_turn 之后有 A 的新回合），这些回合**不丢不合**，
    自动拆成一个新会话「<原标题>（在 A 上的分叉）」，像同步盘的冲突副本。**不做交错合并**：
    两段各自成立的对话硬拼在一起，历史语义和前缀缓存都会坏。
- `running` 回合不同步；`recover_stale_running_turns` 与排队清理改成**只处理本机 device_id 的行**。
- 换机后第一轮：`<host-environment>` 等系统侧内容随设备变化，属于**计划内冷启动**一次（AGENTS §1.6）。
- 中转线续传表是本机的，换机后自动走现有的整段重放兜底（`cli_relay/mod.rs:188`）。

**记忆 —— 补全局 ID + 各整理各的**

- facts / episodes / memory_revisions 追加 `gid TEXT UNIQUE`（`<device>:<ulid>`），迁移末尾纯增量
  （AGENTS §3.1）。本地自增 ID 继续本地用，**日志里一律用 gid**，应用时再翻译成本地 ID
  （`source_episode_ids` 等 JSON 引用同理）。
- 事实的新增/改写/删除 = 以 gid 为键的 `State`，HLC LWW；删除留墓碑。改写历史 `memory_revisions` = `Event`。
- **召回计数、强度、衰减不同步**：它们是本机使用痕迹，每次读都改，同步只会制造噪音。
  各机按自己的使用情况衰减。
- **整理器只整理本机产生的日记**（episode 的 gid 前缀 = 本机）。不需要选主，也不会重复整理；
  整理产物（长期事实）照常同步给所有人。
- 向量不同步，收到新事实后本机回填（已有后台回填 `memory/semantic.rs:305`）。
- 重置记忆 = 一个带 epoch 的全库 `Event`；清 `sqlite_sequence` 只影响本地 ID，不影响 gid。

**账本 —— 近乎现成**

- 条目本身就是文本 ID + revision + 软删除 → 以 ID 为键 HLC LWW。
- 默认账本/账户/分类**改为确定性 ID**（按名字 hash），两台播种出同一个 ID，名字唯一索引不再冲突；
  存量随机 ID 的默认项在首次入网时按名字合并一次。

**文件类（配置、人格、技能、脚本、表情库、profile、身份）**

- 以相对 `~/.gqy` 的路径为键，`State` + 内容 blob，HLC LWW。表情库 `index.json` 按条目合并（ID 是内容 hash）。
- `config.jsonc` 里有**设备相关项**（监听地址、平台开关、沙盒放行清单等），拆一个不同步的
  `config.local.jsonc` 覆盖层；API 密钥是否同步见决策 D4。

**不同步（每台自己的）**：`state/` 下的运行态（闹钟、后台任务、web 登录令牌、续传表、spill、
子代理检查点、提示词指纹、人格提醒缓存）、`cache/`、所有向量与 FTS 索引、知识库索引、
被逐出库 `evicted_context.db`（本机派生）、平台消息历史。

### 3.5 只能执行一次的事：平台宿主设备

- 空间级设置 `platform_host = <device_id>`：只有这台启动 QQ / 连接器接入端、定时消息、主动私聊。
  默认 = 最先配置平台的那台；可在 WebUI / `gqy sync` 里改（改了就是一次显式交接）。
- 平台会话产生的回合照常同步，所以在别的设备上能看到 QQ 聊天记录。
- 闹钟、后台任务：在哪台设就在哪台响/跑，不同步。
- goal 自动轮：跟着会话 owner 走；接管会话即接管 goal。聊天室：跟着房间 owner 走。
- 聊后复盘：只在跑回合的那台排（现状就是如此）。

### 3.6 日志清理

所有成员的 `cursor.json` 都越过某个段后，该段可删；删前 owner 设备写一份该域的压缩快照
（只保留每个键最新的 `State` 和尚需保留的 `Event`），新设备入网先读快照再追日志。
长期不上线的设备会卡住清理 → 超过 N 天未发布 cursor 的设备标记「过期」，回来时走全量快照重建。

## 四、否决的备选

- **同步 SQLite 文件本身**：损坏风险（§3.3 定论）且两边都改过就只能二选一。
- **cr-sqlite 等 CRDT 扩展**：要求每张表 CRDT 化，自增主键不兼容，还要引 C 扩展进 Nix 打包；
  而且回合交错合并本身就是错的语义（见 §3.4）。
- **照搬 Operit2 设备直连**：无常开机器时两台很少同时在线；它的冲突与防重执行也没收敛。
- **每条 SQL / 触发器级复制**：捕获到的是本地自增 ID 与瞬态中间态，合并时全要翻译，得不偿失。

## 五、对缓存与提示词的影响

- 回合副本逐字节落库，owner 端与副本端重放的前缀一致，§1.1/§1.2 契约保持。
- 换设备后 system 侧（host-environment、沙盒摘要）变化 → 每次换机一次计划内冷启动。
- spill 文件路径已化石化在工具输出里，换机后读不到那个文件。建议 `read` 工具对
  「另一台设备的 GQY_HOME 前缀」做路径翻译，并同步 spill 文件为 blob（决策 D6）；不改化石字节。
- 模型可见的新文本只有分叉会话的标题一处，无新注入。

## 六、分期

| 期 | 内容 | 单独有用吗 |
|---|---|---|
| P0 | 设备 ID；`turns`/队列加 `device_id`，陈旧回合与队列清理只认本机；`platform_host` 配置位 | 是，先把「别机 PID 误判」的坑填上 |
| P1 | 中转 trait + 文件夹后端、加密、`gqy sync init/invite/join/status`、日志段与 outbox/inbox、文件类数据同步 | 是：人格、技能、配置两台一致 |
| P2 | 会话与回合：owner/epoch、回合包、接管、分叉、blob | 核心体验 |
| P3 | 记忆：gid 迁移、LWW、本机日记本机整理、向量回填 | 是 |
| P4 | 账本、todos、usage 明细、表情库 | 是 |
| P5 | 日志清理与快照、WebDAV / S3 后端、WebUI 同步状态面板、TUI 状态 | — |

每期独立可验收。P0 很小，建议先做。

## 七、验收要点（草拟）

- 两个 `GQY_HOME` 沙箱指向同一个中转文件夹，模拟两台设备（AGENTS §5.4）。
- A 聊三轮 → B 同步后看到完全一致的回合（逐字节比对 context_messages）→ B 接着聊 → A 同步后停写、看到 B 的回合。
- A、B 离线各自在同一会话里聊 → 合并后得到原会话 + 一个分叉会话，无丢失。
- 两台各写记忆 → 双方都有对方的事实；只有产生日记的那台跑了整理（查辅助用量）。
- 非 `platform_host` 的设备不连 QQ、不发定时消息。
- 中转文件夹里 grep 不到任何明文。
- 第二台换机后第二轮 cache-usage 的 cache_read 正常（§1.6）。

## 八、需要你拍板

| # | 问题 | 选项 | 推荐 |
|---|---|---|---|
| D1 | 中转放哪 | a. 同步盘文件夹（iCloud / 坚果云 / Dropbox 客户端）<br>b. 内置 WebDAV / S3 客户端<br>c. 两者都做 | **a 先做**，trait 留好，b 放 P5 |
| D2 | 同一会话两边离线各聊了 | a. 拆成分叉会话<br>b. 后写者覆盖（丢一边）<br>c. 按时间交错合并 | **a** |
| D3 | 第一批同步范围 | a. 配置/人格/技能 + 会话 + 记忆<br>b. 再加账本<br>c. 全部含平台消息历史 | **a**，账本随 P4 |
| D4 | API 密钥 | a. 同步（端到端加密）<br>b. 不同步，每台自己填 | **a**，并允许 `config.local.jsonc` 按机覆盖 |
| D5 | WebUI 成员账号（多用户）的数据 | a. 不同步，只同步你本人的<br>b. 一起同步 | **a**（成员共用一个库，靠 `sessions.owner` 区分，一起同步要先拆库） |
| D6 | 换机后读旧 spill 路径 | a. 同步 spill + 路径翻译<br>b. 不管，读失败就重跑命令 | **b** 先不管，真碰到再做 a |
| D7 | 开工顺序 | a. 先 P0（小、独立有用）<br>b. P0+P1 一起 | **a** |
