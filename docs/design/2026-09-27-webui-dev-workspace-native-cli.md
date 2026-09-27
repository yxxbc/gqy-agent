# WebUI 开发模式：选文件夹 + 原生 CLI 客户端（方案稿）

> 状态：**方案稿，待用户确认后开工**｜日期：2026-09-27｜对应 `docs/plan/backlog.md` §3
>
> 参考：Cindy（`makecindy/cindy` `2d4507f`，`packages/maker-core/src/agents`）。

## 〇、用户已定（09-27）

| 问题 | 决定 |
|---|---|
| 选文件夹与 `/sandbox` | **分开**：选文件夹只改工作目录、不锁；想锁再 `/sandbox`，锁定时文件夹必须在沙盒内 |
| 开发模式 + 原生客户端时 gqy 工具 | **只留搜索**（原生工具全开，gqy 只桥接 `web_search`）；人格模式不变 |
| 原生 CLI 自己的会话历史 | **这期就能接续**：列出、导入成 gqy 会话、接着聊 |
| backlog 里「claude/codex/agy 只能在人格模式下调用」 | **不要了**：两种模式都能选这些客户端 |

## 一、现状（09-27 核对）

- **工作目录**：WebUI 回合永远 `cwd: None`（`src/web/turns/mod.rs:405`），`session_scope` 退到 `$HOME`
  （`src/web/sandbox_scope.rs:62-70`）。唯一能指定目录的是 `/sandbox`（管理员、要 Landlock、会锁）。
  `sessions.workspace` 列自 v36 起存的是沙盒根（`src/state/migrations/columns.rs:324`），不是 cwd。
  没有列目录接口，建会话也不能带目录（`src/web/sessions.rs:118-126`）。
- **原生客户端**：四条中转线已能用——claude `-p --output-format stream-json`、codex `exec --json`、
  agy、cline，工作目录都取 `effective_workdir()`；续传靠 `state/relay/sessions.json` 的逐消息哈希链
  （`cli_relay/session.rs`、`cli_relay/mod.rs:141` `ResumePlan`）。WebUI 里只能间接选：启用供应商后在模型
  菜单里出现一节。模型列表不带协议字段（`src/config/provider_ops.rs:155-173`）。
- **工具作用域**：`native_tools` / `gqy_tools` 默认都是 `all`（`src/config/defaults.rs:464-514`），
  只去掉与原生重复的几件（`claude_code/mod.rs:257-265`、`codex/mod.rs:70`）。
- **原生会话历史**：没有任何列出/解析代码。反而有**删除**代码：清空 gqy 会话时会删掉对应的
  claude 转录与 codex rollout（`claude_code/mod.rs:303`、`codex/mod.rs:455`）——导入功能上线后这必须改，
  否则清空一个导入来的会话会删掉用户自己的原生记录。

## 二、Cindy 的做法与取舍

| Cindy | 取舍 |
|---|---|
| 文件夹选择：最近 5 个 + 系统对话框；每会话 `working_dir`；项目会话 / 纯聊天会话；可挂只读额外目录 | 借形态。WebUI 没有系统对话框，改成**服务端目录浏览**；额外目录本期不做 |
| 换目录时把 `~/.claude/projects/<槽>/<id>.jsonl` 复制到新槽（claude 按 cwd 找转录） | **借**，否则换目录后续传必断 |
| 模型旁的 harness 选择器（`ModelHarnessPicker.tsx`） | 借入口 |
| 中途换 harness：确定性交接文本 + 切回时恢复原会话只补增量 | 不需要：gqy 自己持有全量历史，换客户端直接整段重放，比交接摘要更全 |
| Claude 走 Agent SDK 长连接、Codex 走 `app-server` JSON-RPC（为了审批/中断） | 暂不跟：用户不需要审批，每回合起进程的现状够用 |
| 列出/导入原生 CLI 已有会话 | **Cindy 没有**，本方案自己设计（§五） |

## 三、A. 会话工作目录

**后端**
- `sessions` 追加列 `cwd TEXT`（迁移末尾纯增量，AGENTS §3.1；`rows.rs` 按列名读）。
- 决定回合目录的顺序（`sandbox_scope.rs`）：成员会话不变 → 请求自带 cwd（终端）→ **会话 cwd（存在且是目录）**
  → `$HOME`。绑了沙盒时，会话 cwd 必须在沙盒根之内，否则用沙盒根。
- `PATCH /api/sessions/{id}` 接受 `{cwd}`；`POST /api/sessions` 可带 `cwd`。
- `GET /api/fs/dirs?path=` 列子目录（名字、是否 git 仓库），**仅管理员**；会话绑沙盒时只许在沙盒内浏览。
  默认隐藏点目录，可切换。
- `GET /api/fs/recent-dirs`：从各会话 cwd 与已导入原生会话的 cwd 去重，按最近使用排序。
- 换目录时对 claude 线做转录搬迁：把该会话续传表里的 claude 会话转录复制到新目录对应的项目槽
  （槽名规则以探针为准，见 §七）。其他线的续传不依赖 cwd。
- 模型看到目录的途径不变：`<runtime cwd=…>` 在用户消息之后（`src/agent/prompt.rs:211`），换目录不破前缀。

**前端**
- 输入框下方信息行（`#composerInfo`）模型按钮左侧加「文件夹」块：显示当前目录末级名，点开是弹层：
  最近文件夹列表 / 目录浏览（面包屑 + 子目录列表）/ 手输路径。
- 会话列表条目的提示里显示 cwd；新建开发会话时默认沿用上一个开发会话的目录。

## 四、B. 客户端选择器 + 开发模式工具作用域

- 模型列表补 `protocol` 字段；前端在「文件夹」块旁加「客户端」块：gqy / Claude Code / Codex /
  Antigravity / Cline（只显示已启用的）。选中后模型菜单只列该客户端的模型，并把会话模型覆盖切到它的
  默认模型。两种模式都可选。
- 开发模式下原生客户端的默认作用域：原生工具全开；gqy 桥**只暴露 `web_search`**。实现上把桥的
  「排除清单」在 dev 下换成「允许清单」，配置里保留可改（`config/tool_plugins.rs` 加一个值，比如
  `gqy_tools_dev = "search"`），人格模式照旧。
- 工具卡片补 Claude 原生 `Read` / `Edit` / `Write` / `Grep` / `Glob` 的图标与摘要（现在落到通用图标）。

## 五、C. 原生会话：列出、导入、接续

本期覆盖 **Claude Code 与 Codex**；Cline / Antigravity 另排。

**列出**
- `GET /api/native-sessions?cwd=&client=`：
  - claude：读 `~/.claude/projects/<槽(cwd)>/*.jsonl`，跳过子代理侧链；
  - codex：遍历 `$CODEX_HOME/sessions/**/rollout-*.jsonl`（默认 `~/.codex`），首行 `session_meta` 取 cwd；
    按文件 mtime 建索引缓存到 `state/native-sessions-index.json`，避免每次全扫。
  - 返回：id、客户端、标题（首条用户消息截断）、起止时间、轮数、模型、cwd、是否已导入。
- 前端：选了文件夹后，侧栏「开发」分组下多一节「本目录的原生会话」。

**导入**
- `POST /api/native-sessions/import {client, id}`：新建开发会话，cwd = 原生会话 cwd，模型覆盖 = 对应客户端
  供应商（原生会话里的模型可用就用它）。转录逐条转成 gqy 回合：用户正文、助手正文、工具调用写成工具报告
  （展示用）。同一个原生会话重复导入 → 打开已有的那个 gqy 会话。
- 会话上记一份 `native_binding {client, native_id, covered_messages, native_tail}`（新表或 JSON 列，迁移追加）。

**接续**
- 下一轮走中转线时，`ResumePlan` 若在续传表里找不到条目，但会话有 `native_binding` 且客户端一致 →
  直接以 `native_id` 续传，只发 `covered_messages` 之后的增量。回合结束照常记续传条目，之后与普通会话无异。
- 续传失败（原生会话被删等）→ 现有兜底：新开 CLI 会话整段重放导入的历史。
- **外部新增**：用户可能在终端里继续用 `claude`/`codex` 聊同一个会话。每次续传前比对原生转录尾部与
  `native_tail`，发现外部新增的轮次 → 先把它们追加导入成 gqy 回合（标记来源），再续传。
- 换成 gqy 自己的引擎或别的客户端 → 照常整段重放（gqy 持有全量历史）。
- **不删用户的原生记录**：`remove_transcript` / `remove_rollout` 只删 gqy 自己创建的 CLI 会话；
  导入来的一律跳过。

## 六、分期与验收

| 期 | 内容 | 验收要点 |
|---|---|---|
| P1 | A：cwd 列、目录浏览与最近目录接口、前端文件夹块、claude 转录搬迁 | WebUI 选 `~/code/x` 后让她 `pwd`；换目录后 claude 线第二轮仍是续传（日志无 resume miss） |
| P2 | B：协议字段、客户端块、dev 只桥接搜索、原生工具卡片 | 开发会话选 Claude Code：她用原生 Bash/Edit，gqy 工具只剩 `web_search`；人格会话工具不变 |
| P3 | C：列出、导入、接续、外部新增、不删原生记录 | 终端里 `claude` 聊两轮 → WebUI 看到并导入 → 接着聊，claude 那边 `--resume` 同一 id；终端再聊一轮 → WebUI 下一轮前自动补上；清空该 gqy 会话后原生 jsonl 仍在 |

实测都在 `GQY_HOME` 沙箱里做（AGENTS §5.4）；涉及中转线与提示词的改动跑 `test_scripts/refactor-check.sh`。
TUI 本身就按终端 cwd 工作；原生会话导入 TUI 端本期不做（AGENTS §8.1，需你确认可以只做 WebUI）。

## 七、开工前要探针核实的事实

- claude 项目槽命名规则（路径里哪些字符换成 `-`）、jsonl 行结构（`type`、`message.content[]`、`isSidechain`、`cwd`）——用真实转录确认。
- codex rollout 行结构（`session_meta` / `response_item` / `event_msg`）与 `exec resume <id>` 能否续 TUI 交互产生的会话，以及 `--ignore-user-config` 下是否仍读 `~/.codex/sessions`。
- 续传用户原生会话时换上 gqy 的 `--strict-mcp-config` / 系统提示词，claude 是否接受（工具清单变化会不会触发 09-01 那类「工具掉线」误读）。
