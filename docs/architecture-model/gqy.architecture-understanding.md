# 顾清影 gqy · 当前架构模型说明书

**用途**：把「gqy 是什么、边界在哪、有哪几块」一次性说清，作为后续流程分析、依赖影响、部署拓扑、风险评审的共同底座。
**读者**：新接手的工程师、想改工具或改入口的贡献者、需要向外解释架构的人。
**视角**：只写**现状**（as-built），不含目标架构、不含改进建议。质量门禁与风险清单不在本文，走 `risk-quality-reviewer`；文档新鲜度校验走 `architecture-health`。
**生成**：2026-09-26 · `system-modeler` + `c4model`（层序图为 `graphviz`）· 版本基线 `Cargo.toml` v0.7.0，commit `0041621b`。

## 文件与阅读顺序

| 顺序 | 文件 | 回答什么问题 |
|---|---|---|
| 1 | 本文 | 边界、组成、关键关系 |
| 2 | `boundary.svg` / `.png` | 谁在用 gqy，gqy 依赖哪些外部系统（L1） |
| 3 | `processes.svg` / `.png` | 系统落地成几个 OS 进程、数据落在哪个文件（L2） |
| 4 | `layers.svg` / `.png` | 44 个顶层模块的层序，以及 18 条反向依赖挂在哪 |
| 5 | `gqy.structurizr.dsl` | 上面三张图的**文本源**：L1/L2/L3 视图，含 `L3-TurnPipeline` 组件图（只有这里有） |
| 6 | `layers.dot` | 层序图的 graphviz 源；本机未装 graphviz，故图由 `render_svg.py` 出 |
| 7 | `gqy.evidence.md` | 每个节点和每条边的 `file:line` 出处，以及 9 项待验证 |

**谁是真相源**：`gqy.structurizr.dsl` 与 `layers.dot` 是文本源，`.svg` / `.png` 是派生产物 —— 改架构先改 DSL/dot，不要手改图。
**看图**：`.svg` / `.png` 直接双击即可；DSL 用 Qoder 的 Structurizr DSL 视图器打开。
**重新生成图**：`python3 render_svg.py && for f in layers boundary processes; do rsvg-convert -w 1500 $f.svg -o $f.png; done`。层序图不手抄数据 —— `render_svg.py` 直接解析 `arch_dep_check.py` 的 `LAYERS` 与 `arch-dep-waivers.json`，门禁一变图就跟着变。本机没有 graphviz（`dot` 不存在），装了之后可用 `dot -Tsvg layers.dot -o layers.svg` 得到自动布局的版本，两者并存时以 dot 为准。

## 一句话边界

gqy 是**一个 Rust crate、两个可执行文件**（`Cargo.toml:9` 关掉 autobins）。除带 `voice` feature 的 `gqy-voice` 之外，所有形态共用同一个二进制：前台 CLI、常驻 daemon、三个自重生 worker 都是同一个 `gqy` 用不同的 argv + 环境变量长出来的不同角色。它向上接人（终端、浏览器、QQ、iMessage、麦克风），向下接模型供应商与本机工具链，横向接 MCP 服务器。

## 三个人，七类外部系统

- **属主/管理员**：终端前的人，也是 WebUI 第一个账号。Owner 信任级，默认不套沙盒（`src/web/sandbox_scope.rs:173-260`）。
- **成员账号**：邀请码注册，只有自己的 `conversation.db`、工作区和私有 persona，回合套 Landlock（`src/web/sandbox_scope.rs:109-157`）。管理员看不到成员会话。
- **聊天对面的第三方**：QQ / iMessage 里的昵称与正文，按不可信输入处理（`AGENTS.md` §4.1）。

外部系统：**OneBot 端**（反向 WS 连进来）、**iMessage 桥**（macOS 上的 Python LaunchAgent，独立发布、调 `gqy ask`）、**上游模型供应商**、**播报供应商**（MiniMax / 小米 MiMo，远端合成）、**MCP 服务器**、**公网**、**本机工具链**（rg / chafa / sh / git / gh / pacman / notify-send / 中转线 CLI… 全部按 PATH 找，spawn 点见证据索引 §5）。

## 六个进程形态（L2）

| 进程 | 常驻? | 职责 | 门面 |
|---|---|---|---|
| `gqy` CLI | 否 | 入口与一次性客户端：REPL、ask、shellhook、stdio 宿主协议、daemon 启停、两个 TUI、OOBE | 不跑回合 |
| `gqy __daemon` | 是 | **唯一跑回合的地方**：HTTP/SSE + 反向 WS + unix socket + 单 actor | `0.0.0.0:8300`、`core.sock` |
| `gqy-voice` | 可选 | 麦克风、唤醒词、VAD、SenseVoice 本地识别、播放 | 只在 `--features voice` 构建 |
| `gqy __renderer-worker` | 临时 | 长图 Markdown→PNG；空闲 10 分钟自杀；RLIMIT_AS 512MB 仅 Linux 施加 | 长度前缀帧 over stdio |
| `gqy __embedding-worker` | 临时 | 本地向量模型；ONNX 动态库外置，避免 24MB 常驻 daemon | 空闲 600 秒退出 |
| `gqy __alarm-worker` | detached | 闹钟，故意活得比 gqy 长 | job-wake 回调 |

**没有 pid 文件**：daemon 存活靠 IPC `Ping` 加 `/proc/<pid>/{stat,comm}`（`src/ipc/lifecycle.rs:125,168,423-437`）。CLI 与 daemon 之间「一次连接 = 一个回合」（`src/cli/stdio.rs:1-5`）。

数据侧的容器全是文件：每身份一份 `conversation.db`（37 版迁移）、每 persona 一份 `memory.db` 与 `evicted_context.db`、机器级知识库两库、记账自有库、以及 `state/` 与 `cache/` 下一堆 JSON/JSONL。另有一类**不在文件系统里**的东西值得单列：提示词、工具描述、`web/` 静态资源、词表都在编译期进二进制（`build.rs:43-70,76-99,112-160,168-187,207-235`），所以改这些必须重新构建。

## 一个回合怎么穿过去（L3）

十步流水线（`docs/architecture.md:49-73`）在代码里对应这条链：

```
surface  →  admission  →  turnLoop  ⇄  llmClient
（场所层）   （单 actor）      ↓  ↑
                            toolFace → toolExec → sandbox
                              ↑
       promptAsm / ctxFossil ─┘        memorySub（挂接点 A/B/C）
```

三处值得强调的实现事实：

1. **actor 串行的是准入，不是执行**。每个 `StartTurn` 变成一次 `spawn_local`（`src/web/actor/mod.rs:96-106`），所以跨会话并发、同会话在 mpsc 里排队；管理员的互斥明确写了「不依赖 actor 串行」（`:398-402`）。一会话一回合的闩锁在 `src/agent/control.rs:1-7`，三个 Drop 型守卫。
2. **工具面内只有一种并行**：`execute_parallel_task_calls` 只在一次响应里有 ≥2 个 `subagent` 调用时才启用，结果按请求顺序回填（`src/agent/turn_loop/parallel.rs:1-6,26-33`）。
3. **前缀即契约**落在这里：`ctxFossil` 的 append-only 逐字节重放（`src/agent/context.rs:1-13`、`src/agent/history.rs:187-225`），配 `toolFace` 的 stub 档位（真名 + ≤60 字摘要 + 空参数壳，`src/tools/registry/lazy.rs:80-115`），共同保证 `tools` 数组会话内字节恒定。

**persona 是唯一的配置单位**：一个目录 = 提示词 + 关系档案 + 启用集（`src/config/persona_manifest.rs:80-83`）。启用集怎么变成工具面：`compose.rs` 的 `UNITS` 表 34 条，每条一个 `when` 门控 + 可选 plugin id（`compose.rs:62-69,340-374`），JSON 描述整体覆盖 Rust 占位（`registry/spec.rs:391-410`，唯一例外 `load_skill`）。dev persona 的启用集为空（`core_only()` `persona_manifest.rs:103-119`），所以是「只构造启用的」而不是「装了再关」。

## 层序与它的破口

`test_scripts/arch_dep_check.py:48-64` 定义了 8 层 44 个顶层模块，规则是任何「低层 → 高层」的引用都算违规，同层互引不管。`arch-dep-waivers.json` 白名单钉住现状 **18 条反向边、69 处引用**，只许变小不许变大。`layers.dot` 就是把这张表画出来，其中三条最粗：

- `llm → tools` 15 处（中转线复用工具契约）
- `tools → agent` 9 处（subagent / 知识库回调回合引擎）
- `agent → platforms` 8 处（回合引擎反过来认平台）

`llm`（L2 基础设施）同时反向吃 `tools`(L3)、`platforms`(L5)、`render`(L3) 三个上层，是模型里最集中的一簇。这些是**现状登记**，不是建议 —— 要不要拆、怎么拆属 `evolution-planner` 的事。

## 假设与口径

- 模型的静态结构来自读代码；**结构性证据不等于行为证据**。例如 `turn_context_messages` 列存在只证明化石有载体，不证明每个写入路径都无遗漏 —— 化石契约的实现细节仍以 `src/agent/context.rs` 的注释与 `shape_tests` / `refactor-check.sh` 的实测为准。
- 本模型未做任何运行时观察（未跑 daemon、未跑构建），因此并发实际达到的度、缓存命中率、沙盒在真机上的行为，都不作为静态结构事实写进来。
- 沙盒的具体读写路径集合按 `src/web/sandbox_scope.rs` 的代码列出；`docs/architecture.md:174-186` 的散文与此一致，冲突时以代码为准。
- 沙盒只在 Linux 有 Landlock 后端。非 Linux 平台（含 macOS）**带策略时拒绝创建子进程**（`unsupported.rs` 的 `apply()` 回 `ENOTSUP`），不带策略的命令照常执行 —— 即「失败关闭」而非降级裸奔（`src/tools/sandbox/mod.rs:14-18`、`backend.rs:20-40`）。内核没编 Landlock 或被禁时同样失败关闭。沙盒只管文件系统，网络不受限（ABI 4 的 TCP bind/connect 不在 handled 集合里）。

## 待验证（摘要）

完整 9 项在 `gqy.evidence.md` §6。最值得先定的三个：入口表外的 5 个 `StartTurn` 发起方算不算「场所」（U1）；子代理绕开 actor 走 `store.pinned().start_turn()` 到底跳过了什么闩锁（U2）；`persona.toml` 里 `subsystems.persona_reminder` 与 `subsystems.emotion` 两个字段作为门控是否真被读到（U4）。

## 怎么维护

改工具、改入口、改数据归属后，重跑这三步即可让模型跟上：`python3 test_scripts/arch_dep_check.py`（层序与反向边现状）→ 按 `gqy.evidence.md` 的表格核对变化的行号 → 若节点增删再改 DSL。`arch_dep_check.py --tighten` 与白名单文件的变动是层序图唯一的更新入口，不要手抄。

图跟着源走：改完 `gqy.structurizr.dsl` 或白名单后跑 `python3 render_svg.py`（重新出三张 `.svg`），再 `rsvg-convert -w 1500 <name>.svg -o <name>.png` 出 PNG。`render_svg.py` 里的层序图不硬编码模块与边，它解析 `arch_dep_check.py` 和 `arch-dep-waivers.json`，所以门禁一变图自动跟上；另两张图的坐标是手摆的，节点增删要同步改 `PEOPLE` / `EXTS` / `STORES` / `WORKERS` 四张表，并与 DSL 对齐。
