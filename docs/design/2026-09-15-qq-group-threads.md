# QQ 群聊消息线程与关联上下文：实现方案

> 状态：**暂不施工**（实测未见收益，见下方「实测结论」）｜日期：2026-09-15｜可视化：`~/Desktop/群聊消息线程.html`｜测试报告：`~/Desktop/群聊线程对照测试报告.html`
>
> 文中 `文件:行号` 均指 `3b169ef4` 时的代码。标「待决」的地方需要拍板后再动工。

---

## 实测结论与方案调整（2026-09-15）

用本机消息历史库里的真实群聊做了对照测试（脚本 `test_scripts/thread_ab/`，数据不入库）。

**方法**：33 个真实回复轮次（2 个群，8/7–9/9），每轮生成三组上下文：现状平铺 / 轻量线程（§8.2 的折叠，不含关联）/ 完整线程（折叠 + 确定性关联摘要）。每组交给一个全新的顾清影子 agent 作答（`gemini-3.7-flash-medium`，与群聊线上模型一致），再由子 agent 盲评。

| | 现状平铺 | 轻量线程 | 完整线程 |
|---|---|---|---|
| 接对人 | 100% | 100% | 100% |
| 串台 | 1/33 | 0/33 | 1/33 |
| 平均名次（↓） | **1.82** | 2.00 | 2.18 |
| 第一名次数 | **18** | 9 | 6 |
| 群聊记录字数 | 1290 | 868 | 983 |

多人混杂的 27 个用例结论相同。

**发现**

1. 现状几乎不串台。唯一一次（G1-06）是「一句话里顺带回答了另一个人上一轮的问题」。
2. 折叠会切掉有用的全群信息：「总结群里信息」、群主在别处交代的权限、群主题、提问者之前聊过的话题。§8.2 的「无关线程折叠」正是平铺胜出的主要原因。
3. 轻量组胜出的几局多为措辞差异，落在采样噪声范围内（输入完全相同的用例名次也能差 0.5）。
4. §7 的关联规则在真实数据上不准：完整组唯一的串台来自确定性摘要带入无关内容，而真正相关的「群规、权限」没被关联上。
5. 省下的开销有限：每次请求的大头是人格与工具定义（约 2.2 万 token），折叠只让总输入少约 5%。

**调整**

- **C 形态（§4 原推荐）暂不施工**。当前群触发基本是一对一，平铺 + `search_real_chat_history` 够用。
- **保留 A 形态（只加 `thread=` 标记、不折叠）作为备选**。等群更活跃、串台明显增多时再做，并先用 `test_scripts/thread_ab/` 重测。
- 以下各节保留作设计参考，其中 §8.2 的折叠与 §7 的关联需按上述发现修订后才能复用。

**测试局限**

- 样本小（33 例、2 群）；评分与作答同一模型。
- 只模拟最近 25 条记录，没有还原更早的会话回合，并关闭了工具。
- **当前消息块用的是简化格式**（`[Current message]` + 记录行），而线上是 `active_target_prompt` 生成的 `[New messages received this turn]`，已带发送者、引用原作者、@ 对象等坐标（`targeting.rs:330-461`）。线上表现应不差于测试里的平铺组；原计划的「方案 2：当前消息块补发送者事实」大部分线上已具备，未单独测试。
- 目标需求群规模约 1357 人，本机历史库里该群只有 18 次顾清影回复，样本不足。群里另一个机器人被 @ 53 次、回复 42 次，可作为扩充用例的来源（未做）。

---

## 0. 一句话

给群消息**归线程**，给线程之间**建关联**。组装上下文时以「当前线程」为主，关联线程只带**带来源的摘要**，无关线程只留一行省略说明。全程不破坏前缀缓存契约（AGENTS §1.1、§1.2）。

---

## 1. 问题与目标

### 1.1 问题

群里多组人同时说话，有人 @ 顾清影，有人互相回复，回头又来问顾清影。现在所有消息按时间平铺进同一段上下文，模型要自己从几十行里拼出「谁在跟谁说什么」，很容易串台：

- 把 A 话题里的词（「九点」「权限」）带进对 B 的回答；
- 没引用的追问被接到错误的话题上；
- 与自己无关的群友闲聊挤占窗口。

### 1.2 目标

1. **隔离**：回答时，模型主要看到当前话题的完整来回。
2. **关联**：相关话题（哪怕 AI 没参与）能以摘要形式进来，并标明谁说的、在哪条线程、什么时候。
3. **缓存安全**：不插入、不删除已发送字节，只有格式说明这类常量改动带来一次计划内冷启动（§1.6）。
4. **可退化**：向量、判定模型任何一环不可用时，退回纯结构规则，功能不停摆。

### 1.3 非目标

- 不改 QQ 客户端的显示方式（QQ 没有原生话题容器）。
- 私聊不做线程化（私聊本来就是一对一，上下文由 agent 会话承载，见 `inject.rs:695-708`）。
- 第一期不做线程的 WebUI 管理界面。

---

## 2. 现状（代码事实）

| 事实 | 位置 | 对方案的影响 |
|---|---|---|
| 一个群一个 agent 会话，`participant_id` 传 `None`，注释写明「Group history is always shared by the whole group」 | `src/platforms/onebot/turn.rs:616-633` | 默认形态下线程不是会话，只能在**渲染层**隔离 |
| 会话绑定键本身支持 `participant_id` | `src/platforms/scheduling.rs:360-375` | 「线程 = 子会话」的形态有现成入口（见 §9） |
| 群上下文注入：水位之后的增量记录 + 当前消息，拼进本轮 user content，之后逐轮原样回放 | `src/platforms/plugins/real_context/inject.rs:689-845` | 新增的块也要一次生成、落库即冻结 |
| 记录块预算 80 KB，条数 `reply_context_window` | `inject.rs:759-765`、`config/platform_plugins/real_context.rs:29` | 线程过滤后同样的预算能装下更长的有效对话 |
| 记录行格式 `[time] sender [msg=id]: content`，附 `reply-to:`、`@mentions:` 缩进行 | `history.rs:280-303` | 模型**今天已经看得到**引用和 @，只是没人帮它归并 |
| 格式说明是会话常量，放在 system 的 `<qq-history-format>` | `src/platforms/onebot/identity.rs:431-440` | 线程标记的说明也放这里，改一次冷启动一次 |
| 逐轮控制块走 `turn_system_context`（尾部通道，会化石化） | `plugins/mod.rs:85-93`、`inject.rs:846-887` | 线程块是**数据**不是指令，放 user content 即可（§1.4） |
| 消息表已有 `reply_to_message_id`（带部分索引）、`mentions_json`、`ingress_order` | `message_history/store/schema.rs:49-77`，`SCHEMA_VERSION = 4`（`:7`） | 引用链可以直接查，线程表加在同一个库里 |
| 入站事件带 `reply_to_message_id`、`replied_message`、`mentioned_user_ids`、`mentioned_bot`、`ingress_order` | `src/platform_types.rs:195-222` | 归线需要的信号入站时都有 |
| 插件 hook：`observe_inbound`（所有准入消息，包括不回复的）、`decide_trigger`、`before_turn`、`after_send` | `plugins/mod.rs:352-422` | 归线挂 `observe_inbound`，组装挂 `before_turn`，bot 出站挂 `after_send` |
| judge 小模型管线：lite 池、JSON 输出、重试、单独记用量 | `real_context/judge.rs:71-151` | 模糊归线的裁决直接复用 |
| 本地向量 `bge-small-zh-v1.5-int8`，失败时调用方退回关键词 | `src/embedding/mod.rs:1-10, 141` | 语义匹配可选，不作为前提 |
| 水位存在插件 KV | `real_context/pending.rs:27-47` | 线程状态的小字段也可以放 KV，大表放 history 库 |

**串台根因**：模型拿到的是一段平铺的、多话题交错的记录，线程关系只以 `reply-to:` 行的形式隐式存在，没有任何代码帮它归并或筛选。

---

## 3. 核心概念

| 概念 | 定义 |
|---|---|
| **线程 Thread** | 一组围绕同一件事来回的消息。有根消息、参与者集合、状态（open / closed）、最后活跃时间。 |
| **AI 线程 / 群友线程** | 顾清影参与过（被 @、被引用、发过言）的线程叫 AI 线程，其余叫群友线程。群友线程默认不喂原文。 |
| **归线 Assignment** | 每条准入消息进来时，决定它属于哪条线程，或新开一条，或暂不归线（游离）。 |
| **关联 Link** | 两条线程之间带类型和强度的边，例如同一参与人、同一对象、时间重叠、互相引用。 |
| **摘要 Summary** | 线程的一两句概括，按线程版本缓存，只在被当作关联引入时使用。 |
| **组装 Assembly** | 回合开始时，按「当前线程原文 + 关联摘要 + 省略说明」拼出注入内容。 |

---

## 4. 三种落地形态（待决：选哪种）

| | A 标注式 | B 分会话式 | C 聚焦式（推荐） |
|---|---|---|---|
| 会话 | 仍然一群一会话 | 一条 AI 线程一个子会话 | 仍然一群一会话 |
| 记录块 | 全部照旧，每行加线程标记 | 只有本线程的记录 | 只渲染当前线程 + 关联线程，无关线程折叠成一行说明 |
| 关联 | 靠模型看标记自己判断 | 摘要注入子会话 | 摘要块，带来源 |
| 隔离强度 | 弱 | 强 | 中强 |
| 前缀缓存 | 不受影响 | 每开一条新线程就冷启动一次 | 不受影响（新块一次生成即冻结） |
| 改动面 | 小：`history.rs`、`identity.rs` | 大：会话路由、pending/supersede、`/reset`、记忆、压缩都按会话算 | 中：`inject.rs` 组装 + 新增 thread 模块 |
| 主要风险 | 平铺仍在，只是更好读 | 会话数量膨胀；跨线程的「她自己说过什么」断开；与现有插件大量耦合 | 折叠掉的消息后来变相关，需要检索兜底（已有 `search_real_chat_history`） |

**推荐 C**，并分期推进（§12）：先只做数据层和归线，dry-run 只写日志，用真实群消息把归线准确率跑出来，再打开组装。B 留作 P4 实验：C 的效果不够时再评估。

---

## 5. 数据模型

放在 message_history 的 SQLite 里（和 `messages` 同库，可以直接用 rowid 关联）。`SCHEMA_VERSION` 从 4 升到 5，迁移只增不减，沿用该库现有的迁移风格（`schema.rs:38-47`）。

```sql
-- 线程
CREATE TABLE IF NOT EXISTS threads (
    id              INTEGER PRIMARY KEY,
    platform        TEXT NOT NULL,
    account_id      TEXT NOT NULL,
    group_id        TEXT NOT NULL,
    persona_scope   TEXT NOT NULL DEFAULT 'default',
    root_message_id TEXT NOT NULL,
    has_bot         INTEGER NOT NULL DEFAULT 0 CHECK (has_bot IN (0, 1)),
    state           TEXT NOT NULL DEFAULT 'open' CHECK (state IN ('open', 'closed')),
    participants    TEXT NOT NULL DEFAULT '[]',   -- JSON 数组，sender_id，上限 16
    message_count   INTEGER NOT NULL DEFAULT 0,
    first_at        INTEGER NOT NULL,
    last_at         INTEGER NOT NULL,
    last_row_id     INTEGER NOT NULL              -- 摘要缓存的版本号
);
CREATE INDEX IF NOT EXISTS idx_threads_scope_open
    ON threads(platform, account_id, group_id, persona_scope, state, last_at DESC);

-- 消息归属（一条消息只归一条线程；游离消息不写）
CREATE TABLE IF NOT EXISTS message_threads (
    message_row_id INTEGER PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    thread_id      INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    source         TEXT NOT NULL,   -- reply | bot_reply | mention_bot | mention_user | continuation | semantic | judge | root
    score          REAL NOT NULL DEFAULT 1.0
);
CREATE INDEX IF NOT EXISTS idx_message_threads_thread
    ON message_threads(thread_id, message_row_id);

-- 线程关联（无向边，存成 a < b）
CREATE TABLE IF NOT EXISTS thread_links (
    thread_a   INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    thread_b   INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    strength   REAL NOT NULL,
    kinds      TEXT NOT NULL,        -- JSON：["participant","topic","time","cross_reply","spawned_from"]
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (thread_a, thread_b),
    CHECK (thread_a < thread_b)
) WITHOUT ROWID;

-- 线程摘要缓存
CREATE TABLE IF NOT EXISTS thread_summaries (
    thread_id   INTEGER PRIMARY KEY REFERENCES threads(id) ON DELETE CASCADE,
    last_row_id INTEGER NOT NULL,    -- 与 threads.last_row_id 相等即新鲜
    text        TEXT NOT NULL,
    model       TEXT,
    created_at  INTEGER NOT NULL
);

-- 可选：线程语义向量（沿用 embedding 存储约定：小端 f32 BLOB + model 标签）
CREATE TABLE IF NOT EXISTS thread_embeddings (
    thread_id   INTEGER PRIMARY KEY REFERENCES threads(id) ON DELETE CASCADE,
    model       TEXT NOT NULL,
    last_row_id INTEGER NOT NULL,
    vector      BLOB NOT NULL
);
```

要点：

- `/wipe`、`reset_context`、`delete_history` 删除消息时，`ON DELETE CASCADE` 会带走归属。空线程由维护任务清理（§7.4）。
- 撤回（`recalled_at`）不删归属，但组装时跳过，与现有记录块的做法一致。
- 线程表是**派生数据**：可以随时按 `messages` 重算，所以迁移失败或数据异常时，允许「清空线程表、从最近 N 小时重建」。

---

## 6. 归线算法

### 6.1 挂载点与时序

- **入站**：在 message_history 插件落库之后，由 real_context 的 `observe_inbound`（`plugins/mod.rs:355`）调用 `threads::assign(event)`。准入但不回复的消息也会经过这里，保证群友线程完整。
  - 顺序依赖：归线要求当前消息和被引用消息都已落库。这一点现在已经成立，而且有两层保证：
    1. message_history 在传输层的 `observe_ingress` 就已落库（`message_history/mod.rs:297-303`，由 `onebot/connection.rs:597` 调用），早于准入与所有 `observe_inbound`。
    2. `observe_inbound` 里它还会幂等地再记一次（`:320-325`，靠 `UNIQUE` 约束去重）。插件按优先级从高到低**逐个 await**（`plugins/mod.rs:454-458` 排序，`:589-595` 顺序执行），message_history（300）在 real_context（200）之前。

    实现时加一条测试锁住「归线时当前消息已有 row_id」。
- **出站**：bot 发出的消息在 `after_send`（`plugins/mod.rs:415`）按回复目标归入对应线程，并把线程标记为 `has_bot = 1`。
- **并发**：同一个群的归线串行执行（沿用 `runtime.rs` 每群一份状态的模式），以 `ingress_order` 为序，避免异步查询让后到的消息先归线。

### 6.2 规则（按优先级，命中即停）

```
R0  bot 自己的出站消息
    → 归入本回合的当前线程（组装时已确定），source=bot_reply

R1  带引用（reply_to_message_id 有值）
    → 被引用消息有归属：归入该线程，source=reply
    → 没有归属（太老、未记录、游离）：
        以被引用消息为根新开线程（用 replied_message 补根信息），
        再把当前消息归入，source=root+reply
    → 被引用线程已 closed：重新打开

R2  @了 bot，没有引用
    → 候选打分（§6.3）：
        top1 是 AI 线程且 score ≥ assign_min_score        → 归入，source=mention_bot
        top1 是群友线程且 score ≥ assign_min_score         → 新开 AI 线程，
                                                          并建立 link(新, top1, kind=spawned_from)
        top1 与 top2 分差 < ambiguous_margin 且开启 judge  → 交给 §6.4 裁决
        都不够分                                          → 新开 AI 线程

R3  @了别人，没有引用
    → 发送者与被 @ 者同在某条 open 线程：归入，source=mention_user
    → 否则新开群友线程

R4  普通消息（没 @、没引用）
    → 同一发送者在 continuation_seconds（默认 90s）内在某线程发过言：归入，source=continuation
    → 否则候选打分，score ≥ ambient_min_score（阈值比 R2 高）：归入，source=semantic
    → 否则游离（不写 message_threads）
```

R2 里「top1 是群友线程就新开 AI 线程并挂关联」这一条，对应可视化 04 的推演：小明「@顾清影 你要打瓦吗」与群友线程「打瓦」内容匹配，但 AI 不在那条线程里。直接并进去会把群友闲聊整体变成 AI 线程，因此改为新开线程，把打瓦线程作为强关联引入。

### 6.3 候选打分

候选集：同群、同 persona、`state = open`、`last_at` 在 `candidate_window_minutes`（默认 120）内的线程，按 `last_at` 取最近 `open_thread_max`（默认 20）条。

```
score = w_struct · S_struct + w_lex · S_lex + w_sem · S_sem + w_time · S_time

S_struct ∈ [0,1]  发送者是参与者 0.5；被 @ 的人是参与者 +0.3；线程 has_bot 且消息 @bot +0.2
S_lex    ∈ [0,1]  当前消息与线程最近 k=6 条的字符 bigram Jaccard（中文友好，零依赖）
S_sem    ∈ [0,1]  embedding 余弦，映射到 [0,1]；向量不可用时该项权重并入 S_lex
S_time   ∈ [0,1]  exp(-Δt / τ)，τ = 15 分钟
```

默认权重（待实测调整）：`w_struct 0.35 / w_lex 0.25 / w_sem 0.25 / w_time 0.15`，`assign_min_score 0.45`，`ambient_min_score 0.6`，`ambiguous_margin 0.08`。

实现注意：

- 线程的比较文本 = 最近 k 条消息拼接，长度截到 512 字节。向量按 `(thread_id, last_row_id)` 缓存，线程每新增 3 条才重算，避免每条消息都跑一次嵌入。
- `S_lex` 先去掉 @ 段、表情占位、URL，再算 bigram。
- 所有打分都是纯函数，便于用固定对话写金标准测试（§11）。

### 6.4 模糊时的 judge 裁决（可选）

只在 R2 候选接近时触发，复用 `judge.rs` 的 lite 池、超时、重试和用量记账（`judge.rs:79-131`），另起 `request_scope = "qq-thread-assign"`，用量单独记一个 kind。

输入（英文机械文本，数据字段原样）：

```
Current message: <sender> <text>
Candidate threads:
t12 participants=[小明, 顾清影] recent:
  小明: ld: symbol not found for arm64
  ...
t15 participants=[小明, 阿杰] recent:
  阿杰: 今晚打不打瓦
  ...
Return JSON: {"thread": "t12" | "t15" | "new", "confidence": 0-1}
```

置信度低于 0.6，或调用失败、超时，都退回打分结果。它不影响是否回复，只影响归属。

### 6.5 线程生命周期

- `last_at` 超过 `thread_idle_minutes`（默认 30）→ `closed`。关闭是惰性的，候选查询时顺手更新。
- closed 线程被引用（R1）→ 重新 open。
- 单群 open 线程超过 `open_thread_max` → 关闭 `last_at` 最旧的。
- 维护任务：每小时清理超过 `thread_retention_days`（默认与消息历史保留期一致）且没有消息的线程。

---

## 7. 关联

### 7.1 关联信号

| kind | 何时产生 | 基础强度 |
|---|---|---|
| `spawned_from` | R2 从群友线程分出新 AI 线程 | 0.8 |
| `cross_reply` | 线程 X 中的消息引用了线程 Y 的消息，但按规则归入了 X（例如 R1 后又被新开） | 0.7 |
| `participant` | 两条 open 线程共享参与者 | 每个共享者 +0.15，上限 0.45 |
| `topic` | 两条线程比较文本的语义或词面相似度 ≥ `link_topic_min` | 相似度 × 0.8 |
| `time` | 两条线程活跃区间重叠（加分项，不单独成边） | +0.1 |

强度合并：`strength = 1 - Π(1 - s_i)`，封顶 1.0。按 `exp(-Δt / 6h)` 衰减，Δt 是距两线程最后活跃的时间。

### 7.2 计算时机

- 某条线程新增消息后，只和同群 open 线程（≤ 20 条）重算这一行边，O(20)。
- `topic` 相似度复用 §6.3 的线程向量或 bigram，不额外调用模型。
- 强度低于 `link_min_strength`（默认 0.3）的边不写。

### 7.3 组装时的选择

当前线程的邻边按 `strength` 降序，取前 `related_max`（默认 3）条，并受 `related_budget_bytes`（默认 1500）限制。强度 < 0.5 的只在预算有余时带上（对应可视化 05 里的强/弱关联）。

### 7.4 隐私边界

- 只在**同一个群内**建关联，不跨群、不跨私聊。
- 摘要只进本群会话的上下文，不进长期记忆（记忆日记读的是 `memory_content`，`plugins/mod.rs:81-84`，不受影响）。

---

## 8. 摘要与上下文组装

### 8.1 摘要

- **生成时机**：惰性。组装时发现关联线程的 `thread_summaries.last_row_id != threads.last_row_id` 才生成。
- **模型**：lite 池，单次调用 ≤ 1 个关联线程，输入该线程最近 ≤ 20 条，输出一句话（≤ 80 字）。同一回合要生成的摘要并发执行，总时长受 `summary_timeout_seconds`（默认 8）约束，超时的走退化方案。
- **退化**：没有模型或调用失败时，用该线程最后 2 条消息各截 40 字拼成摘要。
- **语言**：摘要是数据，保留原语言（中文），外壳标签用英文（AGENTS §1.5 只约束机械文本）。
- **注入边界**：摘要内容来自群友原话，必须过 `safe_prompt_field` / `safe_prompt_string`（`targeting.rs`，AGENTS §4.1）。

### 8.2 组装后的 user content（形态 C）

现在的结构（`inject.rs:805`）：

```
[Prior group chat records]{gap_note}
<增量记录>

<current_block>
```

改成：

```
[Prior group chat records]{gap_note}
<增量记录：只含当前线程、关联线程、游离消息（游离条数有上限）>
(8 messages in 2 unrelated threads omitted. Use search_real_chat_history if needed.)

<related-threads>
- thread=t15 participants=小明, 阿杰 last=21:02 link=spawned_from
  小明和阿杰约了今晚九点打瓦，阿杰在拉人
- thread=t9 participants=老王, 小红 last=20:40 link=topic
  今晚宿舍宽带在维修，预计 11 点恢复
</related-threads>

<current_block，首行带 thread=t18>
```

规则：

1. **记录行加线程标记**：`[time] sender [msg=id] thread=t12: content`。游离消息不加。`<qq-history-format>` 的说明句同步补一句「thread=tN groups messages of one conversation thread」。这是会话常量，改一次冷启动一次（§1.6）。
2. **省略而不是删除**：水位照常推进到本次查询的最高 `ingress_order`，被省略的消息只以一行计数出现。之后它们变得相关时，会通过关联摘要或检索回来。这和现在「超出一块装不下就告诉她自己略读了」（`inject.rs:741-746`）是同一种语义。
3. **一次生成，落库冻结**：`<related-threads>` 在本轮生成后跟随 user content 化石化，之后逐字节回放，满足 §1.2。
4. **不写行为指令**：只陈述「这是什么」。`inject.rs:790-793` 和 `846-856` 的实测结论是：行为禁令没有正面作用，逐轮恒定文本还会永久多带 token。
5. **首轮**：没有水位时仍是全量开场快照，同样按规则 1、2 过滤。

### 8.3 与现有字段的衔接

| 现有 | 改动 |
|---|---|
| `prepare_history`（`history.rs:31`） | 过滤之后再截条数 |
| `format_history_internal`（`history.rs:166`） | 行格式增加可选 `thread=`，参数加 `thread_of: &HashMap<row_id, thread_id>` |
| `context_image_refs`（`inject.rs:810-829`） | 图片引用仍然从全量最近消息收集，不按线程过滤（保证「看刚才那张图」照常可用） |
| `store_reply_watermark`（`inject.rs:838-845`） | 语义不变：按本次实际查询到的最高位推进 |
| judge 的 `history`（`judge.rs:25`） | P3 再按线程过滤，第一期不动 |

---

## 9. 形态 B（分会话）要点：仅作 P4 实验记录

- 路由：`resolve_onebot_session`（`turn.rs:608-624`）的 `participant_id` 传 `Some(format!("thread:{id}"))`，只对 AI 线程这样做，群友消息不开会话。
- 需要同步按会话语义复查的地方：pending/supersede（`pending.rs`）、follow-up 插入、`/reset`、`/wipe`、压缩、会话列表 UI、用量统计。
- 缓存：每开一条新线程，system 前缀可以命中（system 相同），但历史为空，基本相当于冷启动。群聊话题切换频繁时，成本会明显高于 C。
- 连贯性：顾清影在线程 A 说过的话，线程 B 看不到，需要额外的「本群近期 AI 发言摘要」块兜底。
- 结论：只有当 C 的隔离效果实测不够时才做。

---

## 10. 配置项（real_context，新增）

| 键 | 默认 | 说明 |
|---|---|---|
| `thread_enable` | `false` | 总开关。P0 期间打开也只写日志不改注入（见 `thread_dry_run`） |
| `thread_dry_run` | `true` | 只归线、记决策日志，不改组装 |
| `thread_idle_minutes` | 30 | 线程闲置多久关闭 |
| `thread_open_max` | 20 | 单群同时 open 的线程上限 |
| `thread_candidate_window_minutes` | 120 | 候选线程的时间窗 |
| `thread_continuation_seconds` | 90 | 同一发送者连续发言合并窗口 |
| `thread_assign_min_score` | 0.45 | R2 归入阈值 |
| `thread_ambient_min_score` | 0.6 | R4 归入阈值 |
| `thread_judge_enable` | `false` | 模糊时是否调用 lite 模型裁决 |
| `thread_related_max` | 3 | 最多引入几条关联线程 |
| `thread_related_budget_bytes` | 1500 | 关联摘要块预算 |
| `thread_link_min_strength` | 0.3 | 低于此强度不建边 |
| `thread_omit_unrelated` | `true` | 无关线程是否折叠成一行 |
| `thread_ambient_max` | 10 | 记录块里最多保留几条游离消息 |

按 AGENTS §8.1：配置要同步 TUI 表单、WebUI 插件表单和 wiki 13 §7。

---

## 11. 可观测与测试

### 11.1 决策日志

沿用 `decision_log.rs` 的「每个输入都落日志」原则，每条归线写一行：

```
thread-assign msg=123 sender=小明 rule=R2 candidates=[t12:0.31(struct .5 lex .0 sem .2 time .8), t15:0.58(...)] → new t18 link(t15,spawned_from)
```

dry-run 阶段就靠这份日志 + 人工抽查来评估准确率。

### 11.2 测试（AGENTS §5.1：先证明不修时现象会出现）

| 类型 | 内容 |
|---|---|
| 金标准归线 | 可视化里的三组对话（01 串台、04 打瓦、05 关联）写成固定输入，断言每条消息的线程和新建关联 |
| 回归（先红） | 用 01 的 9 条消息跑**现有** `inject_context`，断言记录块含有与当前线程无关的「九点」「权限」行（当前应为真）；上线 C 后断言它们被折叠 |
| 规则单测 | R0–R4 各自的边界：引用了已关闭线程、引用了未记录消息、@多人、连续发言跨窗口 |
| 打分纯函数 | bigram、时间衰减、权重并入（向量不可用时） |
| 缓存契约 | 仿 `agent::tests::context::compaction_resets_the_byte_prefix_at_most_once_each`：两轮群聊请求，第 N 轮是第 N-1 轮的逐元素前缀扩展 |
| 迁移 | v4 库迁到 v5，旧数据可读，线程表为空时组装退化成现状 |
| 退化 | embedding 不可用、judge 超时、摘要失败时仍能组装 |
| 性能量尺（`#[ignore]`） | 单条归线（不含模型）在 20 条 open 线程下的耗时倍率 |

完成后跑 `test_scripts/refactor-check.sh`（涉及提示词组装，是硬要求，AGENTS §5.5），并手测两轮 cache-usage jsonl（§1.6）。

---

## 12. 分期

| 期 | 内容 | 改动文件（预估） | 验收 |
|---|---|---|---|
| **P0 数据 + 归线（dry-run）** | schema v5；`real_context/threads/{mod,assign,score,store}.rs`；`observe_inbound` / `after_send` 挂载；决策日志 | message_history schema、real_context 新模块 | 真实群跑一周，抽查 100 条归线，准确率 ≥ 85%（待决：目标值） |
| **P1 聚焦组装** | 记录行加 `thread=`；无关线程折叠；`<qq-history-format>` 补说明 | `history.rs`、`inject.rs`、`identity.rs` | 01 场景不再串台；缓存两轮测试通过；token 占用对比现状 |
| **P2 关联 + 摘要** | `thread_links`、摘要生成与缓存、`<related-threads>` 块 | `threads/link.rs`、`threads/summary.rs`、`inject.rs` | 04、05 场景回答带来源且不串；关联摘要调用次数与耗时可接受 |
| **P3 协同** | judge 历史按线程过滤；出站引用策略：线程里夹着别人消息时自动引用，让 QQ 侧也形成可见线程 | `judge.rs`、`targeting.rs` | 主动插话不再接错话题；群友看得出她在回哪条 |
| **P4 实验（可选）** | 形态 B 分会话 | `turn.rs` 路由等 | 与 C 做 A/B 对比后再决定是否保留 |

模块划分遵守 AGENTS §6.2（单文件目标 800 行）：新代码放在 `src/platforms/plugins/real_context/threads/` 子目录，不继续堆进 `inject.rs`（已有 937 行）。

---

## 13. 待决问题

1. **形态**：C（推荐）还是直接 B？
2. **默认开关**：P1 上线后 `thread_enable` 默认开还是关？
3. **群友线程对 AI 的可见度**：只给摘要（推荐），还是允许带最近 2–3 条原文？
4. **judge 裁决**：默认关（推荐，先看纯规则准确率），还是默认开？每次都会产生 lite 调用成本。
5. **摘要持久化**：摘要落库是否需要跟随消息历史的保留期与删除命令一起清？（推荐：跟随）
6. **P0 准确率目标**：85% 是否合适，抽查样本从哪些群取？
7. **出站引用（P3）**：主动引用会让群里多出引用气泡，是否接受？

---

## 附录 A：名词对照

| 叫法 | 出处 |
|---|---|
| 话题 / 话题回复、话题群 | 飞书 |
| Threads | Slack、Discord |
| Topics（论坛模式）/ 回复链 | Telegram |
| 引用回复 | QQ、微信（没有话题容器） |
| conversation threading / 按线程隔离上下文 | AI 工程说法 |

## 附录 B：可视化与本方案的对应

| 可视化小节 | 方案章节 |
|---|---|
| 01 问题（平铺 vs 按线程） | §2 根因、§8.2 组装 |
| 03 放进 QQ 的四条规则 | §6.2 R1–R4、§6.5 生命周期 |
| 04 隔着闲聊问「你要打瓦吗」 | §6.2 R2 的 `spawned_from` 分支 |
| 05 线程之间的关联 | §7 关联、§8.1 摘要 |

## 人工批注
- 该项目有旁路请求模式，可以调用旁路请求设置的模型来辅助主模型，帮助其对上下文的整理。
