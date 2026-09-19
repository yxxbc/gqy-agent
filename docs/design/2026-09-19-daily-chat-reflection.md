# 顾清影日常对话的反思机制：纠正记忆 + 聊后复盘

> 状态：**已施工，验收中**（09-20：纠正记忆、认错效果已过；聊后复盘待用户确认）｜日期：2026-09-19｜范围：第一、二期（第三期「交稿前自查」另议）
>
> 文中 `文件:行号` 均指 `4a5c740e` 时的代码。起因案例见 `todolist.md`「顾清影日常对话的反思机制」。

---

## 0. 一句话

让她知道自己错过：用户纠正过的事存成带原因的 `correction` 记忆，随联想记忆召回；一段对话冷下来后，后台用辅助请求复盘，把「下次注意什么」以 `<self-review>` 放进 system 侧，下一轮起生效。人格文本一个字不改。

## 1. 起因（验收用例的来源）

09-19 她写的 ClinePass 争议报告：截图 13 次请求写成 16 次、「40 美元额度」无出处、替用户补了没说过的论点、把对方「按限额百分比推算」写成「按价格推算」。纠错发生在群聊和 Claude Code 里，她没有渠道得知，事后邀功「每个细节和证据都打理得妥妥帖帖」。

## 2. 第一期：纠正记忆

### 2.1 现状

- `remember_fact`（`src/tools/memory.rs:290`）只收 `content`/`source`，`MemoryStore::remember_fact`（`src/memory/write.rs:24`）INSERT 时不写 `memory_type`/`importance`/`tags`，全部落成默认 `fact`/3。
- `facts.memory_type` 允许值 `fact|preference|relationship|task|self|other`，两处校验：`src/memory/validate.rs:161`（整理器）、`src/memory/browse.rs:393`（WebUI 编辑）。
- 联想记忆按 importance 等加权排序（`src/memory/recall.rs:529`），不显示类型。
- 整理器提示词已有「最新的明确陈述或纠正覆盖旧内容」（`src/memory/organizer.rs:312`），但没有纠正这一类。

### 2.2 改动

| 位置 | 改动 |
|---|---|
| `src/tools/descriptions/remember_fact.json` | 新增可选参数 `kind`（`fact`/`correction`，默认 `fact`）与 `reason`。描述补一句英文短句：用户纠正你说的话或做的事时用 `kind=correction`，`reason` 写为什么错 |
| `src/tools/memory.rs` `remember_fact` | 解析 `kind`/`reason`；correction 且缺 reason 时报错「kind=correction requires reason」 |
| `src/memory/write.rs` `remember_fact` | 签名加 `memory_type`、`importance`；correction 落 `memory_type='correction'`、`importance=5`，content 存为「<content>（原因：<reason>）」 |
| `validate.rs:161`、`browse.rs:393` | 允许值加 `correction` |
| `organizer.rs` 整理器提示词 | memory_type 枚举加 `correction`，补一句：用户纠正我的说法或做法时存为 correction，content 写清错在哪、为什么错，importance 给 5 |
| `web/dash-memory.js:32` | `TYPE` 加 `correction: "纠正"`（前端同步，§8.1） |

召回不另做：importance=5 让它在联想记忆里排前面，内容里自带原因。

### 2.3 缓存

`remember_fact` 是 always_loaded，描述/schema 变更 = 一次计划内冷启动（§1.6），改后仍是常量字节。

## 3. 第二期：聊后复盘

### 3.1 触发

- 回合结束（`src/agent/turn_loop/stream.rs:~201`，紧挨 `process_after_turn`）后，若满足：Normal 模式、记忆开启、`crate::paths::is_resident()`（单次 CLI 阅后即焚，不留后台任务），就排一个延时任务。
- 延时 = `memory.review_idle_seconds`（新配置，默认 900 秒，见 §5）。新回合开始即取消，照搬 `start_cache_keepalive` 的取消标志写法（`src/agent/setup.rs:199`）。
- 延时要不短于缓存寿命：复盘结果会改 system 侧，system 一变后面整段历史缓存全废。只在缓存本来就凉了之后换，才几乎不多花钱。

### 3.2 复盘请求

- 新增 `AuxRole::ChatReview`（`src/config/provider.rs:166`），走 `from_aux_role` + `with_request_scope("chat_review")`，独立缓存与用量记账（§1.7），写法照抄 `organize_batch`（`src/memory/organizer.rs:270`）。
- 输入：本会话最近 N 轮（默认 12）的用户/助手原文 + 该用户的 correction 记忆（最多 5 条）。
- 检查项：读错情绪或意图、无条件附和、无据断言、对没核实的工作打包票、重复犯已纠正过的错。
- 输出严格 JSON：`{"notes":["..."]}`，0–3 条，每条英文短句、不超过 120 字符，写成「下次注意什么」而不是「你错了」。无问题就空数组。
- 复盘提示词是模型可见机械文本：英文短句（§1.5）。

### 3.3 存储

- 会话库 `MIGRATIONS` 末尾追加一张表（§3.1 纯增量）：`session_reviews(id, session_id, created_at, last_turn_id, notes_json)`。每次复盘插一行，读取取该会话最新一行。
- 空 notes 也插一行：表示「复盘过、没问题」，下一轮据此撤掉旧提示。

### 3.4 注入

- `prompt.rs` 新增 `with_self_review`，在 `assemble_system_prompt` 链的**最末**追加（append-only，不改既有顺序），内容：`<self-review>` + notes 逐行 + `</self-review>`。
- 只读最新一行；notes 为空则什么都不加。复盘之间字节恒定。
- 走 system 侧、每请求重组、不化石（§1.4），不进 turn history。人格文本不动。

### 3.5 覆盖范围

默认只对属主会话（终端 / WebUI）生效。QQ 群聊一个会话里多人混杂，复盘容易把 A 的反馈套到 B 身上，先不开（见 §5）。

### 3.6 可见性

WebUI 记忆页新增只读的「复盘」栏（用户 09-20 选定位置）：按当前人格列出各会话的历次复盘，新→旧；同一会话只有最新一次标「生效中」，notes 为空的标「无需调整」，更早的标「已被替换」。接口 `GET /api/dash/memory/reviews`，仅管理员（复盘只对属主会话开）。纠正记忆本就在「事实」栏，类型显示为「纠正」。

## 4. 风险与对策

| 风险 | 对策 |
|---|---|
| 过度道歉、变拘谨 | notes 写成「注意什么」；提示词明示不要求在对话里认错 |
| 反思变表演（OOC） | `<self-review>` 在 system 侧，复盘提示词要求 notes 是行为指引不是台词 |
| 复盘误判 | 最多 3 条、每次复盘整体替换上一版，错的会被下一次冲掉 |
| 费用 | 只在冷却后跑一次、输入限 N 轮、`with_max_tokens` 封顶，用量单独记账可查 |
| 缓存 | 见 §2.3、§3.1 |

## 5. 已定（09-19 用户确认）

1. `memory.review_idle_seconds` 默认 900（15 分钟），可在设置里改；0 = 关闭复盘。
2. 复盘只覆盖属主的终端与 WebUI 会话；QQ 私聊与群聊都不开。
3. 允许本地跑测试与构建，`refactor-check.sh` 全绿后再交人工验收。

## 6. 验收

1. 自动：新增单测覆盖 correction 写入与校验、复盘 JSON 解析（空/超长/非法）、`with_self_review` 字节稳定与空 notes 不注入；`refactor-check.sh` 全绿；手测两轮请求的 cache-usage jsonl，第二轮 cache_read 不异常下降（§1.6）。
2. 人工（重放起因案例）：
   - 告诉她「那份报告有错：次数是 13 不是 16，40 美元没出处」→ 她调用 `remember_fact(kind=correction)`，WebUI 记忆页能看到「纠正」类条目。
   - 冷却时长过后，日志里有一条 `chat_review` 辅助请求，`session_reviews` 有新行。
   - 下一轮再提那份报告 → 她承认算错而不是邀功，语气仍是她自己。
