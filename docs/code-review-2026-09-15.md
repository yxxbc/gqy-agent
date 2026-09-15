# 可扩展性评审：为「频繁增添改功能」做架构适配（2026-09-15）

只读评审。范围：`src/` 全部 724 个 `.rs`（290,954 行）、`web/`（约 30k 行 JS/CSS）、`test_scripts/` 门禁。

方法：git 提交历史量化（1108 次提交）→ 三个子代理并行深挖（工具系统 / 分层依赖 / 入口与平台重复）→ 实跑项目自有门禁（`arch_dep_check.py`、`refactor_size_report.py --check`）。所有结论带 `文件:行号` 或命令输出佐证。

---

## 零、总判断

**方向是对的，纪律罕见地好；问题不在分层，在接线。**

`docs/architecture.md` 描述的三层、persona=preset、字节契约三件事在代码里基本成立，而且配套的机器守护比多数商业项目更严：字节契约有 `shape_tests.rs` 指纹夹具、文件规模有门禁、依赖方向有门禁、2356 个测试。规模门禁实况：拆分进度 83.4%，超红线 2000 行的文件只剩 1 个。

所以**不需要重画分层**。真正卡住"频繁增添改功能"的是另一件事：

> **中心文件是手工维护的清单，且没有任何机器能拦住漏掉的同步。**

加一件东西要在 8-23 处手工登记，编译器不报错、测试不报错、门禁不报错——漏了就在运行时静默降级。

但这件事**不是全局的**。把十种典型改动全测一遍（见 §六），结论是：

> **四条路径烂，三条路径优秀。** 供应商、slash 命令、平台插件三处已经是好设计
> （分别只需 0 / 1 / 1 处改动），而工具、插件、子系统、数据库四处是 23 / 13 / 内联 / 38 处。

更关键的是：**那三条"优"各自代表一个已经被验证过的正确模式，而且都在这个仓库里**
（见 §七）——`REPL_COMMAND_TABLE` 的单一常量表、`PlatformPlugin` 的全默认方法 trait、
`GqyDash.register` + 图标名遍历测试。团队已经踩过"两份清单分叉"和"静默失败"的坑，
并且已经各解决过一次。

因此本次评审的建议不是"引入新抽象"，而是：

> **把已经验证过的三个样板，从 3 处复制到另外 4 处。**

这比"重新设计架构"便宜得多，也比"继续手工维护"可靠得多。

---

## 一、根因一：接线税（最高频路径的实测成本）

### 1.1 加一个工具 = 碰 23 个文件

最近一次真实加功能是 `3b169ef4`（github 工具）。改动面：

```
23 files changed, 1406 insertions(+), 3 deletions(-)
```

其中功能本体是 `src/tools/github/` 5 个文件（约 1023 行）。剩下的是接线：

| 接线点 | 文件 | 改了什么 |
|---|---|---|
| 描述契约 | `src/tools/descriptions/github.json` | 新建 70 行 |
| 描述契约 | `src/tools/tool_descriptions.rs` | **补 1 行 `include_str!` 宏** |
| 注册 | `src/tools/mod.rs` | `compose_registry` 加一个 if 块 |
| 注册 | `src/tools/mod.rs` | 可读名表加 1 条 |
| 契约夹具 | `src/tools/fixtures/registry-shapes.json` | 重新生成 |
| 配置 | `src/config/mod.rs` | 新增 `tools.github.{enabled,coauthor_name,coauthor_email}` |
| 宿主接线 | `src/agent/mod.rs`、`agent/setup.rs`、`agent/turn_loop/redo.rs` | 3 个文件 |
| 宿主接线 | `src/cli/args.rs`、`cli/mod.rs`、`cli/github_cmds.rs` | 3 个文件 |
| 宿主接线 | `src/llm/openai_compatible/builder.rs` | 1 个文件 |
| 共享状态 | `src/tools/workspace.rs` | `TurnModel` task-local |
| 文档 | 4 个 md + `next-release-note.md` + `.test-count` | |

**约 9 处是纯接线**，与功能本身无关。

### 1.2 加一个插件 = 手工同步 13 处

以 `"memes"` 这个插件 id 为例，字符串出现在：

```
src/tools/mod.rs:337                    可读名表
src/tools/mod.rs:533                    compose_registry 块
src/config/persona_manifest.rs:35       PLUGIN_IDS
src/config/feature_catalog.rs:28        引导可开关列表
src/config/feature_catalog.rs:51        plugin_label 显示名
src/web/accounts_api.rs:49              成员插件白名单
src/config_tui/plugins.rs:142           TUI 插件列表
src/tools/descriptions/manage_meme.json:70   分组
src/tools/descriptions/use_meme.json:45      分组
src/tools/descriptions/groups.json:17        分组显示名
web/dash-memes.js:492                   dashboard 注册
web/app.js:10786                        面板列表
src/paths/resources.rs:20               资源枚举
```

### 1.3 根因：`compose_registry` 是 171 行手写 if 链

`src/tools/mod.rs:465-636`。每个插件一个 block：

```rust
if plugin("memes") && config.plugins.memes.enabled {
    memes::register(&mut registry, config.clone(), paths.clone());
}
if plugin("album") {
    album::register(&mut registry, config.clone(), paths.clone());
}
// … 18 个同类 block
```

注册 API 本身设计得不错（`ToolSpec::new` + builder + `apply_built_in_description`），问题在于**没有一张表**。加工具必须手改这个函数体。

---

## 二、根因二：扩展点靠约定，不靠接口

### 2.1 全库 290k 行只有 5 个 trait

```
src/voice/stt.rs:9                     SttEngine
src/platforms/plugins/mod.rs:281       PlatformPlugin
src/platform_types.rs:426              PlatformAdapter
src/platform_types.rs:535              PlatformToolContext
src/llm/openai_compatible/lower.rs:346 IntoNonEmpty（工具性小 trait）
```

平台层有真接口（`PlatformAdapter` / `PlatformToolContext`）——这是"加一个平台"最贵的路径，已经抽象过了。但：

- **没有 Tool trait**（靠 `ToolSpec` 闭包，尚可）
- **没有 LLM trait**（`Agent` 直接持有具体类型）
- **没有 Memory trait**（`Agent.memory: MemoryStore` 具体类型）
- **没有子系统 trait**

后果：扩展靠"记得改对地方"，编译器帮不上忙。

### 2.2 三个挂接点是内联硬编码，不是回调

`docs/architecture.md` 说子系统经"挂接点 A（联想注入）/ B（逐出库归档）/ C（回合后钩子）"进入。代码实况：

- **A** `src/agent/turn_loop/stream.rs:100-129` — 直接调 `self.memory.association_with_semantic(...)`
- **C** `src/agent/turn_loop/stream.rs:208-218` — 直接调 `self.memory.process_after_turn(...)`
- 同一函数里还有内联的人格提醒（`:25`）、表情包提醒（`:130-139`）、化石化（`:145-148`）

**没有任何 hook 注册机制**。加一个子系统 = 改 `chat_stream_turn` 这个 231 行的函数。

### 2.3 `Agent` 是 43 个字段的对象

`src/agent/mod.rs:237-332`（95 行）。持有 `client`、`memory`、`tools`、`state`、`config`、`paths`、平台上下文、任务上下文、各种 `Option` 开关……

---

## 三、根因三：半成品重构的代价（`AgentMode`）

`docs/plan/2026-09-10-layered-architecture.md` 的定论是"**mode 概念退场**，persona 就是 preset"，施工记录里标 `[~]`（部分完成）。

实况：**336 处引用 / 61 个文件**，而且仍在做行为分支：

```
src/agent/prompt.rs:49-50           Dev → dev_system_prompt / Normal → system_prompt_with
src/agent/prompt.rs:101,114         mode != Dev 决定是否注入
src/agent/turn_loop/mod.rs:61       artifact_auto_publish 取决于 mode
src/agent/turn_loop/mod.rs:81       mode == Normal 分支
src/web/turns/task.rs:310-311       选 normal_tools 还是 dev_tools
src/web/turns/task.rs:363           local_webui && mode == Normal
```

配套代价：`AgentTurnControl` 同时持有 `normal_tools` 与 `dev_tools` **两份完整 ToolRegistry**（`src/agent/control.rs:154-155`）。

**这是最贵的债**：现在判断"我在哪个人格/哪个上下文"有两个真相源（persona 清单 和 `AgentMode`），每个新功能都要在两者之间做选择，而且选错不报错。

> 半成品重构比不改更贵——不改只有一个概念，改一半有两个。

---

## 四、根因四：门禁的盲区

### 4.1 依赖方向门禁只覆盖 8 条边

`test_scripts/arch_dep_check.py` 的 `FORBIDDEN` 表列了 8 条禁止边。实际存在**约 200 条跨模块依赖**，即约 190 条不在管辖内。

实跑输出（当前通过，因为白名单冻结了债务）：

```
tools → web          5 处    ← 白名单
tools → platforms    5 处    ← 白名单
platforms → web      4 处    ← 白名单
web → cli            4 处    ← 白名单
render → cli         2 处    ← 白名单
tools → cli          2 处    ← 白名单
web → config_tui     2 处    ← 白名单
llm → platforms      1 处    ← 白名单
```

未被管辖的重要反向依赖（可继续增长而不被拦截）：

| 边 | 处数 | 为什么是问题 |
|---|---|---|
| `llm → tools` | 15 | LLM 客户端层依赖工具层（`tools::workspace`、`tools::sandbox`、`tools::builtin_registry`） |
| `platforms → agent` | 13 | 场所层反向依赖核心 |
| `agent → platforms` | 12 | 核心依赖场所层 |
| `render → tools` | 12 | 渲染层依赖工具层 |
| `render → llm` | 12 | 渲染层依赖 LLM 层 |
| `state → llm` | 12 | 状态层依赖 LLM 层 |
| `runtime → tools` | 12 | 运行时依赖工具层 |
| `tools → agent` | 11 | 工具层反向依赖核心 |

门禁的规则表模型也与文档不一致：文档说 `platforms` 是最上层"场所层"，门禁里 `platforms` 与 `agent` 是**同层**（都禁止 use web/cli/config_tui）。

### 4.2 静默失败点（三处，全部无测试守护）

**① `tool_descriptions.rs` 宏漏行 → 静默降级**

`AGENTS.md §2.1` 已警告："新增必须补宏行，忘了=JSON 静默失效"。守护实况：

```rust
// src/tools/registry/spec.rs:392
if let Some(desc) = crate::tools::tool_descriptions::get(&self.name) {
    // 缺失时不进入此块，Rust 侧占位描述照用，无日志无报错
}
```

当前 `descriptions/*.json` 46 个、宏内 `include_str!` 46 行，**靠人手工对齐，无测试**。
更糟：`shape_tests.rs` 夹具不能发现这个——漏行后工具以占位描述注册，作者会顺手用
`write_registry_shape_fixture` 刷新夹具，**把错误状态固化进基线**。

**② `plugin_label` 有静默兜底，且已漂移**

```rust
// src/config/feature_catalog.rs:59
_ => ("", ""),
```

`PLUGIN_IDS` 18 个 id 中，`album` / `map` / `express` **没有标签**（15/18 覆盖）。
当前被 `src/web/accounts_api.rs:492` 的 `if label.is_empty() { id.as_str() }` 兜住，
且这三个不在 `TOGGLE_PLUGINS` 里，所以暂不可见——但把 `map` 移进 `TOGGLE_PLUGINS` 就会在引导里显示空行。

**③ 前端与 Rust 两套配置默认值，无测试比对**

```js
// web/settings-schema.js:3-5
// 一切默认值/范围/枚举都抄自 Rust 侧(src/config/**),中文标签抄自 TUI
// 配置器(src/config_tui/**)的 t("English", "中文")。改 Rust 默认值时请同步。
```

3513 行的 `settings-schema.js` 是数据驱动的（好设计），但**与 Rust 侧的默认值靠人同步**，
`src/`、`tests/`、`test_scripts/` 里没有任何比对测试。

### 4.3 位置索引耦合（同类 bug 家族）

- `src/config_tui/settings.rs:98-113`：位置读回 + `debug_assert_eq!(fields.len(), 15)`。
  注释自承："an insert in the middle silently writes every later value into the wrong setting."
  但 `debug_assert` 在 release 不生效，且只查数量、不查顺序。
- `src/config_tui/platforms/mod.rs:538,541,544`：`index if index == 23 - usize::from(!parallel)`
  —— 索引随条件字段漂移的魔法数。

**这个家族已经咬过一次**：`docs/code-review-2026-08-16.md` 第 4 轮 §1.4 记录
`config_tui.rs:389` 的 `index == 13` 让 `api_quota` 账号管理界面**永不可达**，整组编辑函数成死代码。

---

## 五、其他实测发现

### 5.1 死代码

```rust
// src/agent/turn_loop/stream.rs:26-32
// 人类新回合:重复链语境重置。goal 自动续轮/job 唤醒不算语境
// 变化——跨自动轮的原样重复正是最需要打断的死循环(dsh 同款:
// 只有 user 来源消息重置链)。
if matches!(
    crate::tools::workspace::current_turn_origin(),
    crate::tools::workspace::TurnOrigin::Human
) {}
```

**空块**。重置逻辑已移走，条件与解释性注释留下——注释还在描述一个不存在的行为，会误导后来者。

### 5.2 前端 `web/app.js` 仍是单文件巨石

- **13,266 行**，一个 IIFE，**513 个顶层函数**
- 全项目最热文件：累计 146 次提交；09-10 分层重构后仍是 **64 次**（同期 `src/tools/mod.rs` 18 次）
- 最长函数 `visualPixelsToLayout` 515 行、`bindEvents` 307 行、`createTool` 239 行

前端已有部分模块化（`dash-*.js` 14 个文件 + `shared.js` / `diff.js` / `linkcards.js` 等），
但核心对话流、工具卡渲染、事件订阅全在 `app.js` 里。**这是全项目改动成本最高的单点。**

### 5.3 文档漂移

`AGENTS.md §5.5` 要求"涉及 agent/llm/registry/提示词的改动，`scripts/refactor-check.sh` 五道门禁是验收硬要求"。
实际路径是 `test_scripts/refactor-check.sh`（`9f8ed256` 于 09-14 改名 `scripts → test_scripts`）。
新来的 agent 按文档跑会找不到脚本。

同一脚本内的注释也已过时：称"`cargo fmt --check` 当前有约 4400 行 diff（历史遗留）"，
而 `AGENTS.md §5.5` 说仓库自 08-26 起 fmt-clean。

### 5.4 负面结论（确认做得好的部分）

- **字节契约守护是真的**：`src/tools/shape_tests.rs` 对 normal/dev/restricted 三个面
  逐工具算 sha256 并与夹具比对，漂移即红，且提供了刻意的刷新入口（`#[ignore]` 的
  `write_registry_shape_fixture`）。这是本次评审见到的最高质量守护。
- **文件规模纪律好**：724 文件 / 290,954 行，中位数 327，P90 837；超 800 行 82 个（11%），
  超红线 2000 行仅 `src/render/stream/timeline.rs`（2400）。
- **测试基数扎实**：2356 个用例；`refactor-check.sh` 数"跑了多少个"而非"过了多少个"，
  并明确拒绝豁免（注释里复盘了 PTY 用例被错误豁免的教训）。
- **平台层抽象到位**：`PlatformAdapter` / `PlatformToolContext` 是真接口，
  `docs/architecture.md` 第四节的"场所只声明两属性"在代码里有对应结构
  （`Surface { trust, interactive_questions }`，`src/tools/mod.rs:431`）。
- **作者对分层倒置有自觉**：`src/tools/mod.rs:796-800` 明确记录了一次下沉修复
  （`build_tool_registry` 从 `cli.rs` 搬到 `tools`，一次断掉 `web→cli` 与 `tools→cli` 两条边）。

---

## 六、加功能成本表（全路径实测）

"频繁增添改功能"不止加工具一条路径。把十种典型改动全部测一遍，问题分布很不均匀：

| 要加的东西 | 要碰几处 | 评级 | 证据 |
|---|---|---|---|
| 一个 LLM 供应商 | **0 处代码** | 优 | `ProviderConfig` 全字段 `#[serde(default)]`（`src/config/provider.rs:271`），纯配置驱动 |
| 一条 slash 命令 | **1 处** | 优 | 往 `REPL_COMMAND_TABLE` 加一项，Tab 补全 / 前缀解析 / `/help` / WebUI `GET /api/commands` 四处自动跟上（`src/slash_commands.rs:76`） |
| 一个平台插件 | **1 处** | 优 | `impl PlatformPlugin`（17 方法全带默认实现，只需 `descriptor()`），8 个真实实现者 |
| 一个 dashboard 面板 | 6 处 | 中 | `dashboards/mod.rs` 的 `pub mod`、新 `x.rs`、`server.rs` 路由表（memes 有 6 条）、`web/dash-x.js`、`index.html` 的 script + rail 按钮、`app.js` 面板清单 |
| 一个配置项 | 4 处 | 中 | Rust 结构体 + `settings-schema.js` + `config_tui` + 位置索引读回 |
| **一个工具** | **23 个文件** | 差 | `3b169ef4`；9 处纯接线 |
| **一个插件** | **13 处** | 差 | `"memes"` 字符串散落 13 个手工清单 |
| **一个子系统** | 改内联函数 | 差 | 挂接点 A/B/C 是 `turn_loop/stream.rs` 里的具体调用，无注册机制 |
| **改 DB 字段** | **38 处 SELECT** | 差 | `map_turn_row` 用位置索引（`src/state/conversation_db/rows.rs:46`） |
| 一个平台 / 入口 | 未知 | 未知 | `PlatformAdapter` 只有 1 个真实实现（`onebot/adapter.rs:56`），其余 11 个全是测试替身，无第二例可参照 |

**这张表的读法**：不是"全局都烂"，而是**四条路径烂、三条路径优秀**。
问题集中在工具 / 插件 / 子系统 / 数据库四处，而供应商、命令、平台插件三处已经是好设计。

---

## 七、已有的三个正确样板（这是本次评审最重要的发现）

前三行"优"不是巧合——它们各自代表一个已经被验证过的正确模式，而且都在代码里：

### 样板 1 · 单一常量表驱动多消费者（`REPL_COMMAND_TABLE`）

`src/slash_commands.rs` 的文档注释把这件事说得比我能说得更好：

```
//! 原本长在 `cli::repl::commands` 里，是 `pub(in crate::cli)` 的——WebUI 够
//! 不到，只能自己再维护一份清单，两份迟早分叉（加一条命令忘了改另一边）。
//! 提到 crate 级之后 CLI 与 WebUI 同源，`GET /api/commands` 直接从这张表出。
```

**团队已经踩过"两份清单分叉"的坑，并且已经解决过一次。** 一个 `const REPL_COMMAND_TABLE`
驱动 Tab 补全、前缀解析、`/help` 输出、分发、WebUI 命令列表五处。

`compose_registry` 的 171 行 if 链、`PLUGIN_IDS`、`feature_catalog::plugin_label`、
`builtin_readable_tool_name`（68 条）、`config_tui/plugins.rs`、`accounts_api.rs`
——这六份手工清单要做的事，与 `REPL_COMMAND_TABLE` **完全同构**。

### 样板 2 · 全默认方法的扩展 trait（`PlatformPlugin`）

`src/platforms/plugins/mod.rs:281`：17 个方法**全部带默认实现**
（`Ok(())` / `false` / `Box::pin(async {})`），只有 `descriptor()` 必需。
插件只覆盖自己关心的钩子，`register_tools(&mut ToolRegistry, Arc<PlatformTurnContext>)`
自己往工具面加东西。

8 个真实实现者（reply_processor / access_manager / meme_collector / scheduled_messages /
real_context / message_recall / group_management / message_history）证明这个设计**能扩展**。

这就是"加一个子系统"应该长的样子——而不是去改 `chat_stream_turn` 的内联调用。

### 样板 3 · 注册机制 + 守护静默失败（`GqyDash.register` + 图标名遍历测试）

`web/dashboards.js:376` 的 `register({ name, mount, refresh })` 是正经的注册机制。
更值得学的是它对静默失败的处理——顶部注释记录了两次真实事故：

```
// 展开态的箭头一直缺着:目录树折叠时画 chevron-right、展开时要 chevron-down,
// 表里没有就静默画出一个空 <svg>,于是展开之后箭头凭空消失(09-09 用户反馈)。
// …09-09 顺着 chevron-down 一并补齐,并加了一条测试遍历各面板实际用到的
// 名字,以后漏一个会当场报红。
```

**"静默失败 → 补测试让下次报红"这个闭环，团队已经走通一次。**

### 结论

本次评审的建议不是"引入新抽象"，而是：

> **把已经验证过的这三个样板，从 3 处复制到另外 4 处。**

---

## 八、建议

### P0 · 纯防御（低成本、零架构风险、当天可完成）

**① 补三张一致性测试，把静默失败点变成红灯**

| 测试 | 断言 | 实现成本 |
|---|---|---|
| 描述契约完整 | `descriptions/*.json` 的文件名集合 == 宏内 `include_str!` 集合 | 一行目录扫描 |
| 插件标签完整 | `PLUGIN_IDS` 每个 id 的 `plugin_label` 非空 | 一个循环 |
| 配置 schema 同步 | `settings-schema.js` 的字段路径集合 ⊆ Rust 配置字段路径集合 | 解析 JS + serde 遍历 |

这三张测试直接消灭第四节全部三个静默失败点。**投入产出比最高。**

**② 清死代码 + 修文档路径**
- 删 `src/agent/turn_loop/stream.rs:26-32` 的空 if 块
- `AGENTS.md §5.5` 路径改 `test_scripts/refactor-check.sh`
- `refactor-check.sh` 头部关于 fmt 的过时注释

**③ 依赖门禁改"全序层表"，盲区归零**

把 `arch_dep_check.py` 的 `FORBIDDEN` 从"手列 8 条边"改成一张**层序表**
（例：`i18n/paths/config < llm/state/memory < tools/render < agent < platforms < web < cli/config_tui`），
全对比较。现存约 200 条一次性写进白名单，此后只减不增。

效果：现在能挡住"新增反向边"，但挡不住"在盲区里新增反向边"。改完全部挡住。

**④ 数据库行映射改列名读取（照抄同文件已有的正确写法）**

这是 AGENTS §3.1 自认的"全库最脆弱处"，实况量化：

| 项 | 现状 |
|---|---|
| `turns` 的 22 列清单 | **在 `history.rs` 里复制了 7 遍**（:78, :98, :118, :137, :190, :223, :249） |
| `map_turn_row` 的读取方式 | **32 个位置索引** `row.get(0)`…`row.get(21)`（`rows.rs:46`） |
| `turns` 的列清单常量 | **不存在** |
| 加一列的代价 | 改 7 处 SELECT + 在 `map_turn_row` 插入位置 + **其后所有位置索引顺移** |

而同文件里 `sessions` 已经是正确写法：`SESSION_COLUMNS` 常量（`rows.rs:10`）+ 4 处
`format!("SELECT {SESSION_COLUMNS} FROM sessions …")` + `session_record_from_row` 的
**11 个列名读取** `row.get("session_id")?`。

**修法（机械、可证行为等价）**：

1. 加 `pub(crate) const TURN_COLUMNS: &str = "turn_id, seq, …"`，照抄 `SESSION_COLUMNS` 的样子
2. `history.rs` 的 7 处内联列清单换成 `format!("SELECT {TURN_COLUMNS} FROM turns …")`
3. `map_turn_row` 的 32 个 `row.get(N)` 换成 `row.get("turn_id")?`

收益：加一列从"改 7 处 + 位置顺移"降到"改 2 处（常量 + 映射）"，且位置索引这个 bug 类别**整体消失**。
全库 38 处 `FROM turns` 已确认**没有一处用 `SELECT *`**，所以列名读取可行。

### P1 · 降接线税（中期，1-2 周）

**④ `compose_registry` 改声明式注册表**

把 171 行手写 if 链换成一张表：

```rust
struct PluginUnit {
    id: &'static str,
    label: (&'static str, &'static str),   // 中文名 + 一句话
    always_on: bool,
    enabled: fn(&AppConfig) -> bool,
    register: fn(&mut ToolRegistry, &AppConfig, &GqyPaths),
}
const PLUGINS: &[PluginUnit] = &[ /* 18 行 */ ];
```

`PLUGIN_IDS`、`feature_catalog::plugin_label`、`TOGGLE_PLUGINS`、`compose_registry` 的 block、
可读名表全部从这张表派生。**加一个插件从 13 处降到 1 处。**

> 这不是新设计——就是**样板 1**（`REPL_COMMAND_TABLE`）的同一手法，
> 只是把"一张表驱动五个消费者"从 slash 命令复制到插件。

**⑤ 可读名与分组收口**
`src/tools/mod.rs` 的 `builtin_readable_tool_name` 有 **68 条手工 match**，
与 `descriptions/*.json` 的 `display_name` / `groups.json` 职责重叠。收口到单一真相源
（可挂在 ④ 的 `PluginUnit` 上，也可直接由 `descriptions/*.json` 派生）。

**⑥ 拆 `web/app.js`**
按已经验证过的 `dash-*.js` + `GqyDash.register` 模式（**样板 3**），
把 513 个顶层函数按域切成 8-12 个文件：对话流 / 工具卡 / 输入框 / 事件订阅 /
滚动 / 用量 / 设置 / 语音。**这是全项目最热文件，收益最直接。**

### P2 · 结构性（需单独排期，动核心）

**⑦ `AgentMode` 退场收尾（336 处 / 61 文件）**
必须先清干净。这是当前最大的概念债：两个真相源并存，每个新功能都要选，选错不报错。
建议顺序：先让 `AgentTurnControl` 只持一份 registry（按 persona 清单现场算），
再把 298 处引用机械替换为 persona 查询，最后删枚举。

**⑧ 三个挂接点抽 trait**
定义 `SubsystemHooks { before_request, after_turn, on_evict }`，
让"加一个子系统"从"改 `stream.rs`"变成"实现一个 trait + 注册一行"。

> 直接照 **样板 2**（`PlatformPlugin`）的形状：方法全带默认实现，只需 `descriptor()`，
> 外加一个 `register_tools` 钩子。这个设计已被 8 个实现者验证过。

**⑨ `Agent` 43 字段拆 `AgentCore` + `TurnState`**
不变的（client/paths/config）与每回合可变的（messages/usage/context）分开。

---

## 九、推荐执行顺序

```
第 1 步（当天）   ① 三张一致性测试 + ② 清死代码与文档   ← 推荐先做
第 2 步（1-2 天） ③ 依赖门禁改全序层表
第 3 步（2-3 天） ④ 数据库行映射改列名读取（TURN_COLUMNS）
第 4 步（1 周）   ⑤ 声明式注册表（顺手吃掉 ⑥）
第 5 步（1 周）   ⑦ 拆 app.js
第 6 步（排期）   ⑧ AgentMode 退场 → ⑨ 挂接点 trait → ⑩ Agent 拆分
```

理由：第 1-3 步是**纯防御 / 机械改造**——不改行为、不碰字节契约、不触发缓存冷启动，
却能把"漏改不报错"这个根因堵死，并消灭全库最脆弱处；第 4-5 步把最高频路径
（加工具 / 加插件 / 改前端）的接线税从 13 处降到 1 处；第 6 步才是动核心的
结构性改造，需要单独排期与验收。

**不做的事**：不重画目录、不改三层命名、不动提示词与缓存契约、不引入新依赖。
现有分层是对的，缺的是把约定变成机器可检查的接口与表——
而这三张表的形状，仓库里已经有现成的正确样板。
