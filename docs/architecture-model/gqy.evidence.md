# gqy 架构模型 · 证据索引

生成：2026-09-26 · `system-modeler` + `c4model`（层序图另用 `graphviz` 形式）
配套：`gqy.structurizr.dsl`（C4 L1/L2/L3）、`layers.dot`（模块层序 + 反向依赖）

confidence 取值沿用 `architecture-contract.md`：**high** = 代码/配置/构建直证；**medium** = 多个局部信号一致但无直证；**low** = 从命名或文档推断；**unknown** = 待查。
每条 current-state 节点至少一个 high/medium 来源；不满足的都进了下面的「低置信与待验证」。

## 1. 边界（L1）

| id | 类型 | conf | sourceRefs |
|---|---|---|---|
| `owner` | actor | high | `docs/architecture.md:150-158`；`src/web/actor/mod.rs:398-402`（admin 互斥） |
| `member` | actor | high | `src/state/mod.rs:224-229`（成员库路径）；`src/web/sandbox_scope.rs:109-157` |
| `outsider` | actor | high | `AGENTS.md` §4.1（trusted/untrusted 分离）；`src/platforms/plugins/real_context.rs` 的 `safe_prompt_field`，亦见 `src/lib.rs:67-69` fuzz 入口 |
| `onebot` | external | high | `src/platforms/onebot/connection.rs:236-246,342-343,355`；`src/web/server.rs:709,713` |
| `imsgBridge` | external | high | `scripts/imessage/README.md:6-10`；`imessage_bridge.py:801,823`；`AGENTS.md` §4.5 |
| `llmProvider` | external | high | `src/llm/openai_compatible/`；`src/llm/provider_capabilities.rs:16,33` |
| `mcpServers` | external | high | `src/config/mod.rs:882-895`（servers 字段）；`src/tools/mcp.rs:213-260,393` |
| `ttsProvider` | external | high | `src/web/voice_tts.rs:1-10`（MiniMax t2a_v2 / MiMo chat/completions） |
| `webNet` | external | high | `src/tools/web/mod.rs:37,53`；`src/tools/archlinux/aur_review.rs:229,240`；`src/tools/github/actions.rs:73` |
| `toolchain` | external | high | 见 §5 子进程清单 |

## 2. 进程形态（L2 容器）

| id | 名称 | conf | sourceRefs |
|---|---|---|---|
| `cli` | gqy CLI | high | `Cargo.toml:9,15-17`；`src/main.rs:4,21-25`；`src/lib.rs:74-91`；`src/cli/mod.rs:230-245,310-313,389-393` |
| `daemon` | gqy __daemon | high | `src/cli/args.rs:90`；`src/cli/daemon_cmds.rs:17-42,68-140`；`src/daemon.rs:5-11`；`src/ipc/lifecycle.rs:508-523` |
| `voice` | gqy-voice | high | `Cargo.toml:21-24,97-98`；`src/bin/voice.rs:1-20`；`src/voice/mod.rs:1-6` |
| `rendererW` | __renderer-worker | high | `src/platforms/plugins/renderer/worker.rs:15,18,39,41,71-102,137-140,197-233`；RLIMIT_AS 值 `:23-30`、`#[cfg(target_os = "linux")]` 才施加 `:178-192` |
| `embedW` | __embedding-worker | high | `src/embedding/worker.rs:3-6,26-29,89-110,367`；空闲 600s `:67-71`；RLIMIT_AS 值 `:36-45`、仅 Linux 施加 `:160-175` |
| `alarmW` | __alarm-worker | high | `src/tools/alarm.rs:73`；`src/cli/alarm_worker.rs:1-4`；`src/cli/args.rs:84` |

网络与套接字面：

| 面 | 值 | conf | sourceRefs |
|---|---|---|---|
| IPC | `<runtime>/core.sock`（`$XDG_RUNTIME_DIR`，否则 `state_dir/gqy`） | high | `src/paths/mod.rs:510-520`；`src/web/ipc_server.rs:12-19`；`src/ipc/protocol.rs:127,367` |
| 锁文件 | `core.lock` / `starter.lock` / `state/daemon-launch.json` | high | `src/paths/mod.rs:524,528,532` |
| WebUI HTTP | 默认 `0.0.0.0:8300`，被占则临时端口；`--bind 127.0.0.1` 收窄 | high | `src/ipc/launch.rs:13`；`src/web/server.rs:57-78` |
| SSE | `GET /api/events` | high | `src/web/server.rs:435` |
| 反向 WS | 同端口 `/ws`、`/onebot/v11/ws`；端口不同才另开监听 | high | `src/web/server.rs:709,713`；`src/platforms/onebot/connection.rs:236-246` |
| daemon 存活 | 无 pid 文件，靠 IPC `Ping` + `/proc/<pid>/{stat,comm}` | high | `src/ipc/lifecycle.rs:125,168,423-437` |

## 3. 数据与状态（L2）

| id | 归属粒度 | conf | sourceRefs |
|---|---|---|---|
| `convDb` | 每身份一份 | high | `src/state/conversation_db/mod.rs:160,187`；`src/paths/mod.rs:435-440`；`src/state/mod.rs:224-229`；表清单 `src/state/conversation_db/migrations/baseline.rs:12,27,38,54,77,149,169,399,427,445,478-556,627,653,665,684` + `columns.rs:336` |
| `memDb` | 每 persona 一份 | high | `src/memory/mod.rs:335`；`src/memory/schema.rs:13,30,46,53,61,69,251`；`src/config/persona_paths.rs:191-198` |
| `evictDb` | 每 persona（随 memory 打开） | high | `src/memory/mod.rs:336`；`src/memory/schema.rs:266,277` |
| `ledgerDb` | 自有版本 | high | `src/ledger/mod.rs:67,72`；`src/ledger/schema.rs:61,72,87,109,167,185,195,236` |
| `kbDb` | 机器级 | high | `src/tools/knowledge_base/mod.rs:83-85,307`；`store.rs:66,74` |
| `flatState` | 机器级 | high | `src/state/mod.rs:306,312-317`；`src/state/usage.rs:165`；`src/web/server.rs:144`；`src/paths/mod.rs:533,537`；`src/models_cache/mod.rs:73`；`src/llm/cache_log.rs:22-23` |
| `homeTree` | 混合 | high | `src/paths/mod.rs:148-151,280-288,360,386,392-393,471-476` |

**身份解析链**（模型里最重要的一条边）：principal = `blake3(入口, 账号 id, 用户 id)` 长度前缀编码、取 24 hex → `src/platform_types.rs:50-64`；由回合上下文构造 `src/platforms/turn_context.rs:164-169`；随请求冻结进记忆访问 `src/agent/setup.rs:460-464`、`src/memory/mod.rs:218`。store 路由：`StoreRegistry` `src/runtime/stores.rs:17-24`，`for_owner:41` / `for_identity:58` / `owner_of_session:63`（>8192 清缓存 `:69`）/ `locate_session:85-113`，写在 `src/web/sessions.rs:158,446`。

**迁移**：`MIGRATIONS` `src/state/migrations/mod.rs:29`，37 条、`LATEST_VERSION = 37` `:217`，`PRAGMA user_version` 逐条一事务 `:222-292`。回合读列按名不按位：`TURN_COLUMNS` `src/state/conversation_db/rows.rs:48`、`map_turn_row:50-88`。

**编译进二进制（不是运行时文件系统）**：提示词 XOR+base64 `build.rs:43-70`；工具描述清单 `build.rs:76-99` → `src/tools/tool_descriptions.rs:48,51,53`；`web/` 全量 `build.rs:112-160` → `src/web/embedded.rs:17,28-45`（dev 覆盖 `:47-65,113`）；jieba 词表 `build.rs:168-187`；o200k `build.rs:207-235` → `src/token_counter.rs:148`。

**内存态**：`TurnResourceCache`（8 条 LRU、blake3 配置键）`src/runtime/state.rs:129-201`；`StoreRegistry.members/owners` `stores.rs:21-23`；`PLATFORM_ACCESS_INDEXES` `state.rs:141-162`。

## 4. daemon 内部（L3 组件）

| id | conf | sourceRefs |
|---|---|---|
| `surface` | high | `src/web/server.rs:57-78,144,157-165,435,679,709`；`src/web/ipc_server.rs:12-19,144,614,899,905`；`src/web/sandbox_scope.rs:40-70` |
| `admission` | high | `src/web/actor/mod.rs:15-29,96-106,398-402,676-688`；`src/runtime/actor.rs:14-99` |
| `turnLoop` | high | `src/agent/turn_loop/mod.rs:1161`；`turn_loop/parallel.rs:1-6,26-33,50-51`；`src/agent/control.rs:1-7,60-72,116-132,161-187,190-254` |
| `promptAsm` | high | `src/agent/prompt`；`src/prompts.rs:5-6`；`src/config/persona_paths.rs:33,117`；`src/persona_hint.rs:57,64,103,209` |
| `ctxFossil` | high | `src/agent/context.rs:1-13,26,796-830`；`src/agent/history.rs:82,165-166,187-225`；`src/state/conversation_db/columns.rs:20,67` |
| `toolFace` | high | `src/tools/compose.rs:62-69,80-327`（`UNITS` 34 条）`,340-374,451-461`；`src/tools/registry/spec.rs:391-410`；`src/tools/registry/lazy.rs:80-115,122,140-160`；`src/tools/registry/mod.rs:231-260,301-320,325,358`；`src/tools/shape_tests.rs:10-55,60,68-90` |
| `toolExec` | high | `src/tools/mod.rs:401-423,507,527`；`src/tools/load_tools.rs:66-140,316-344` |
| `sandbox` | high | `src/tools/sandbox/mod.rs:14-18,39-63,66-104`；`linux.rs:24,48-52,91-133`；`backend.rs:20-40`；`src/web/sandbox_scope.rs:23-26,74-83,95-107,109-157,139-145,173-260`；`src/llm/openai_compatible/cli_relay/process.rs` 的 `RelayProcess::spawn → confine_relay`（另见 `docs/architecture.md:187-190`） |
| `llmClient` | high | `src/llm/openai_compatible/*`；`src/llm/provider_capabilities.rs:16,33`；`src/llm/cache_log.rs:22-23`；`src/agent/turn_loop/mod.rs:139-146` |
| `stores` | high | `src/runtime/stores.rs:17-118`；`src/runtime/state.rs:43,75,270-276`；`src/platform_types.rs:50-64` |
| `memorySub` | high | A：`src/agent/turn_loop/stream.rs:86-122`；B：`src/agent/context.rs:796-830` ← `src/agent/history.rs:82`、`src/web/actor/mod.rs:351`；C：`src/memory/write.rs:102` ← `src/agent/turn_loop/stream.rs:201-211` |
| `platformAdapt` | high | `src/platforms/onebot/dispatch.rs:422`；`turn.rs:297`；`src/platforms/turn_run.rs:35,107`；`src/platforms/turn_context.rs:33-38,164-169`；`src/platforms/access_control.rs`；`src/platforms/plugins/scheduled_messages`；`src/platforms/plugins/real_context/{emotion,affection,inject,judge}` |

## 5. 子进程清单（`toolchain` / MCP / 自身 worker 的全部 spawn 点）

`gqy __daemon` `src/ipc/lifecycle.rs:508-523` · `gqy __alarm-worker` `src/tools/alarm.rs:73` · `gqy kb embed reindex` `src/tools/knowledge_base/index.rs:447` · `gqy-voice` `src/web/voice_bridge.rs:211`（设备枚举 `src/web/voice_api.rs:35`、`src/config_tui/voice.rs:119`）· `claude` `src/llm/openai_compatible/claude_code/mod.rs:37` · `codex` `.../codex/mod.rs:41` · `agy` `.../antigravity/mod.rs:111`（三者统一由 `cli_relay/process.rs:94` 起）· `rg` `src/tools/default_tools/files.rs:264,300` · `chafa` `src/tools/vision/print.rs:86,224`、`src/terminal/chafa.rs:85`、`src/render/math/raster.rs:268` · `sh` `src/tools/default_tools/command.rs:44`、`src/tools/jobs/mod.rs:443` · 脚本体 `src/tools/scripts/mod.rs:157` · MCP `src/tools/mcp.rs:393` · `git` `src/default_kb.rs:447,465`、`src/pm/mod.rs:504,517` · `gh` `src/tools/github/actions.rs:73` · `makepkg`/`pacman` `src/tools/archlinux/aur_review.rs:229,240` · `wl-copy`/`xclip` 等 `src/clipboard.rs:108,232,274-293` · `notify-send` `src/notify.rs:60` · `pactl` `src/voice/mic.rs:93` · `fcitx5-remote` `src/config_tui/widgets/mod.rs:76,81` · `$EDITOR` `src/config_tui/widgets/form.rs:583-586` · `open`/`xdg-open` `src/cli/repl/tail/screen/select.rs:189`。
**ffmpeg 未在任何 spawn 点出现**（只有文档注释提到）：`src/platforms/onebot/outbound.rs:131`、`src/tools/vision/mod.rs:480`。

## 6. 低置信与待验证

| # | 事项 | 当前 conf | 验证动作 |
|---|---|---|---|
| U1 | §四 入口表外的 5 个 StartTurn 发起方（goal driver、job-wake、scheduled_messages worker、private_initiative、WebUI voice）是否都算「场所」 | medium（代码在，归类未定） | 逐个确认它们是否共用同一 trust 解析；`src/web/server.rs:157-165`、`src/web/goal_driver.rs:344`、`src/web/actor/job_wake.rs:559` |
| U2 | 子代理走 `store.pinned().start_turn()` 而非 actor（`src/tools/subagent.rs:999,1123`）——它绕过了什么 | medium | 确认 subagent 是否受 `agent/control.rs` 的同会话闩锁约束 |
| U3 | 中转线成员能否读到 CLI 登录态（`docs/architecture.md:190` 自陈的固有取舍） | medium（文档声明，未实测） | 需真机验证，属运行时观察，不做为静态结构事实 |
| U4 | `subsystems.persona_reminder` / `subsystems.emotion` 两个 manifest 字段（`src/config/persona_manifest.rs:39,43`）作为门控是否生效 | low（未读到消费点；实际门控在 `config.prompt.persona_reminder` `turn_loop/parallel.rs:295` 与 `web/member_persona.rs:258`） | grep 消费点或跑门控测试 |
| U5 | `~/.gqy/models/` 是否真存在 | low（`src/paths.rs` 无该派生；只有 `state/models` `src/web/voice_bridge.rs:102` 与 `cache/models_cache.json`） | 查 `docs/architecture.md:131` 与实际目录 |
| U6 | MCP 归 core 还是 plugin：`compose.rs:292-304` 是 `plugin("mcp")` 门控且 dev 面排除（`persona_manifest.rs:100-118`），与 §一「核心工具」列举冲突 | high（代码侧清楚，归类是语义问题） | 定归类即可，无需查代码 |
| U7 | 迁移是否遵守「不回填不删列」：v24 `DROP TABLE goals`、v36 `UPDATE sessions SET workspace=NULL`、v23/v1/v2 drop+recreate、多处 backfill（`src/state/conversation_db/columns.rs:112,131,137,144-162,213-214,328`、`baseline.rs:228-229,250-251,257,339`） | high（代码直证，与 `AGENTS.md` §3.1 表述不一致） | 属文档口径待对齐，本模型只记录现状不判定 |
| U8 | WebUI 默认绑 `0.0.0.0` 与 LAN 可达是否有意为之 | high（`src/web/server.rs:57-60` + 注释） | 安全口径交由 `risk-quality-reviewer`，此处只作为边界事实登记 |
| U9 | `voice` 容器在无 `--features voice` 构建里完全不存在，此时语音入口的行为 | unknown | 需按 feature 组合验证 |
