# 独立桌面悬浮窗（她的 Live2D 皮）

> 2026-09-28 用户拍板：`docs/plan/2026-09-14-selection-menu.md` 那一轮先 commit（`8bad5e45`），
> 接着做**独立的桌面悬浮窗**，不做 WebUI 里的 Live2D。
> 本文是施工前的方案稿：现状、选型、拆分、风险。**标 `未实测` 的地方施工时补数字**，
> 需要用户拍板的点集中在 §2，不自己定。遵循 `docs/理念.md`。

## 0. 一句话

桌面上放一个**透明、无边框、置顶**的小窗，里面是她的 Live2D 形象：待机呼吸与眨眼、
说话时嘴动、按情绪换表情、点她有反应。它是一条**旁路**：不碰对话上下文、不改缓存契约、
关掉它 daemon 照常跑。

## 1. 目标与非目标

### 目标（v1）

| # | 目标 | 说明 |
|---|---|---|
| 1 | 一个能拖、能记住位置的置顶小窗 | 透明背景，只有人物本身占鼠标；关掉不留垃圾 |
| 2 | 待机演出 | 呼吸、眨眼、视线跟随鼠标（Live2D 的自动动作 + 参数驱动） |
| 3 | 说话有口型 | 她在播报/出字时嘴动；嘴形与音量挂钩是 v2 |
| 4 | 情绪映射表情 | 用现成的 `valence / arousal` 选表情，不是玄学 |
| 5 | 点她有反应 | 单击一个动作或一句短话；双击开 WebUI；右键菜单换模型 / 置顶 / 关掉 |
| 6 | 不影响主链路 | 不新增任何进主对话前缀的东西（AGENTS §1） |

### 非目标（v1 明确不做）

- 不在悬浮窗里聊天（输入框与消息流仍归 WebUI、TUI）。
- 不做三平台同时铺开：**先 macOS**，Windows / Linux 随后（见 §2 第 1 条）。
- 不自己实现 moc3 变形与物理（见 §4 B 案）。
- 不把模型资源编译进二进制：模型是用户资产，放磁盘（见 §5.3）。
- 不做「常驻满帧」：不可见即停渲染（见 §8）。

## 2. 必须你拍板的五件事（推荐项已标）

| # | 问题 | 选项 | 我的推荐 |
|---|---|---|---|
| 1 | 平台范围 | (a) 只 macOS (b) macOS + Windows (c) 三平台一起 | **(a)**：先把链路打通；Linux 牵扯 webkit2gtk 系统库与 Nix 发布链（§7），是另一摊活 |
| 2 | 模型从哪来 | (a) 官方示例模型（Hiyori / Haru 等，仅测试与个人非商用）(b) 你已有的皮 (c) 拿现有立绘找绑定师做 | **(a) 起步**：示例模型把链路验完再换皮。**代码变不出 Live2D 模型，绑骨骼是美术活** |
| 3 | 依赖 | 是否允许新增 `wry` + `tao`（+ objc2 四件套），独立 bin + 独立 feature | **允许**：唯一务实路线（§4 A 案） |
| 4 | 口型精度 | (a) 状态驱动（说话就动，v1）(b) 包络驱动（按音量逐帧，v2） | **(a) 起步，(b) 列步 5**：包络要动 `gqy-voice` 与 IPC 帧，v1 不值得 |
| 5 | 发布包带不带 pet | (a) 带（含 Cubism Core，包变大）(b) 不带，用户自己 `cargo install --features pet` | **(b) 起步**：先不塞进 Nix 与四平台发布，免得连带改发布链 |

## 3. 现状（读码事实）

| 事实 | 位置 |
|---|---|
| **重功能走独立 bin + feature** 的现成先例：主 `gqy` 永不链接 sherpa-onnx，语音前端是 `gqy-voice` | `Cargo.toml`（`[[bin]] gqy-voice` + `required-features = ["voice"]`） |
| 没有任何窗口 / GUI 代码，没有 wry / tao / gtk 依赖 | 全仓库无命中；`nix/package.nix` 无 webkit / gtk |
| 事件总线是**带 id、可重放**的记录流，订阅端可按 `after` 续读，断代要 `resync_required` | `src/web/server.rs` 的 `events()` + `state.events.subscribe_after(after)` |
| HTTP 那条路要身份（`require_identity`）；**IPC 那条路天然是本机信任** | `src/web/server.rs`、`src/ipc/*` |
| IPC 已有「附着到一个正在跑的回合、把事件帧推到底」的命令形状 | `src/ipc/protocol.rs` 的 `Command::FollowRun { run_id }` |
| 播报发生在 `gqy-voice` 进程里，播报起止回报 `voice.speaking {on}` | `src/voice/speaker.rs`、`src/voice/worker.rs` 头部表 |
| 情绪是**二维数值 + 标签**，不是枚举：`valence / arousal` + `label_for()` | `src/web/dashboards/affection.rs`、`src/platforms/plugins/real_context/emotion.rs` |
| 状态目录已存在（`~/.gqy/state/`） | `src/paths/mod.rs`（`state_dir`） |
| 前端静态资源全量编译进二进制；`build.rs` 扫目录自动收录 | `web/README.md`、AGENTS §7.1 |
| 打包与发布链要两处同步：`publish-release.yml` 与 `nix/package.nix` | AGENTS §7.3 |
| 新顶层模块要登记层序表与架构文档 | `test_scripts/arch_dep_check.py`、`docs/architecture.md`（AGENTS §8.2） |

## 4. 选型：四条路

### A（推荐）`wry` + `tao` 的无边框透明窗，渲染 Live2D Web SDK

窗口是 Rust 起的，内容是 WebView 里的一个本地页面：Cubism Core（wasm）+ `pixi-live2d-display` 负责画，
我们只管窗口、通道、参数。

- 好处：Live2D 官方运行时与生态都在 Web 侧，模型（`.model3.json` / `.moc3` / 纹理 / 动作 / 表情）
  拿来就能用；前端技能与 WebUI 同一套；口型、表情、物理全由 SDK 管。
- 代价：新依赖树（macOS 上是 objc2 全家桶）；Linux 要 webkit2gtk；WebView 进程常驻内存（§8）。
- 硬约束：全离线、CSP `script-src 'self'`，SDK 与模型都不走 CDN。

### B 纯 Rust 自绘

自己解析 `.moc3`、做网格变形 / 遮罩 / 物理。

- 好处：无 WebView，体积与内存最小，与 TUI 一套渲染思路。
- 代价：`.moc3` 是专有二进制格式，Rust 侧没有成熟实现，等于从零写一个 Live2D 运行时，
  数月工程且要跟着 Live2D 版本走。**不推荐**。

### C 不做独立进程，塞进 WebUI

浏览器不给无边框 / 透明 / 置顶（`documentPictureInPicture` 也不给）。**排除**。

### D（折中，作为 A 的步 1）先做「静态立绘悬浮窗」

同 A 的窗口与通道，先把现有立绘（`assets/mascot/portrait.png`）放进透明窗，
用 CSS / Canvas 做呼吸、淡入、轻微视差；口型与表情先用切图顶。

- 好处：窗口 / IPC / 配置 / 拖动 / 位置记忆这些**真正容易翻车的部分**先落地并验收，
  换 Live2D 只换渲染层；万一 Cubism 授权谈不下来，产品也已经能用。
- 代价：不是真 Live2D。

**推荐路线：D → A**（步 1 做 D，步 3 换成 A 的渲染层）。

## 5. 架构

### 5.1 进程

- 新 bin `gqy-pet`（`src/bin/pet.rs` + `src/pet/`），`--features pet` 才构建；主 `gqy` 不链接 wry。
- 生命周期：`gqy pet` 启动（第一次自动 `ensure_daemon`，与 REPL 同路）；窗口关掉即进程退出。
- **不在 daemon 里开线程画窗口**：daemon 是长驻服务，窗口崩了不该拖垮它。

### 5.2 通道：走 IPC，不走 HTTP

宠物要的是「现在在说话吗 / 在想什么 / 情绪如何」，这些都在 daemon 侧：

- 新增 `Command::SubscribePet`（名字施工时定）：附着后 daemon 推裁剪过的事件帧，形状照 `FollowRun`。
- 推什么（**只推宠物用得上的**）：`turn.started` / `turn.finished`、`assistant.delta`（说话中）、
  `reasoning.delta`（思考中）、`voice.speaking {on}`（口型开关）、情绪 `valence / arousal` 变化。
- 为什么不用 `/api/events`：那条路要登录身份（`require_identity`）。给本机进程开免登录口子
  是安全面的事，不值得为一个悬浮窗开。IPC 的 socket 在 `~/.gqy` 下，权限即边界。
- 反向：宠物把「窗口位置 / 缩放 / 当前模型」写回配置。

### 5.3 资源与配置

- 页面本体（HTML / JS / CSS）编译进二进制（照 `web/` 的先例，`build.rs` 扫目录）。
  页面放哪：`web/pet/` 会被 `test_scripts/web_dep_check.py` 的模块方向检查覆盖，
  `src/pet/web/` 走 `include_str!` 则不进那套检查。**施工时选一个并在稿子里记下来**。
- 模型走磁盘：`state_dir/pet/`（备选：人格目录——换人格换皮）。**施工时再定**，v1 先全局一套。
- 配置新增一节 `display.pet`：`enabled`、`model`、`scale`、`always_on_top`、`position`、
  `click_through`、`idle_fps`。WebUI 设置页加「桌面悬浮窗」卡片（前后端两边都要改，AGENTS §8.1）。

### 5.4 窗口行为

- 透明、无边框、置顶可选；拖动 = 按住人物拖，位置与缩放写回配置。
- **点击穿透**：人物轮廓之外不挡鼠标（`set_ignore_cursor_events` + 命中测试；
  糙一点就用矩形判定）。施工验收标准是「能不能点到底下的窗口」。
- 多屏与缩放：记住屏幕 id + 相对坐标，屏幕拔了要能回主屏。
- macOS 细节（`未实测`）：`canJoinAllSpaces`（所有桌面都在）、不占 Dock（Accessory 激活策略）、
  与全屏应用的共存行为。**先写最小探针把这些行为验一遍再谈施工**。

## 6. 口型、表情、情绪

- 口型 v1：`voice.speaking on/off` + 说话时按固定节奏切嘴形参数（够「像在说话」）。
- 口型 v2（步 5）：播报段按 20ms 窗算 RMS，随 IPC 推给宠物（**不推原始音频**）：
  数据量小、不重复解码，也不会让宠物变成第二个播放器（重复播报是事故，不是功能）。
- 情绪：`valence / arousal` → 表情集。做法是在 `emotion.rs` 的标签体系上加一层「表现映射」
  （`label` → 表情名），宠物只认表情名，不认数值细节。
- 思考中：`reasoning.delta` 期间给「思考」小动作或头顶小气泡（与 WebUI 的「正在思考」签同一个语义）。

## 7. 打包与平台（连带更新）

| 平台 | 需要做的事 |
|---|---|
| macOS | `cargo install --path . --features pet`；不出新系统依赖。发布包带不带 pet 见 §2 第 5 条 |
| Linux | `webkit2gtk-4.1`（Nix 里加依赖，`nix/package.nix`），本地构建要装 `-dev` 包 |
| Windows | WebView2 运行时（Win10+ 自带），构建无额外系统库 |
| CI | `ci.yml` 加一条 `--features pet` 的编译检查（不跑窗口测试），别让矩阵膨胀 |
| 发布链 | 若要随包发布：`publish-release.yml` 打包步骤与 `nix/package.nix`、`nix/prebuilt.nix` 的 `wrapProgram` **两处同步**（AGENTS §7.3） |
| 供应链 | `deny.toml` 核对 wry / tao 依赖树的许可（MIT / Apache-2.0 / BSD 都在白名单内，`未实测`，要跑一次 `cargo deny check`） |
| 文档 | `docs/wiki/03-使用方式.md`、`docs/wiki/05-配置指南.md`、`docs/architecture.md`、层序表 `test_scripts/arch_dep_check.py` |

**授权（要你确认）**：Live2D Cubism Core 是专有许可，条款对「随应用分发」有明确边界；
仓库对名字与形象是单独授权（`LICENSE-ASSETS`）。官方示例模型只适合打通链路，
真要发布要先读条款并写进 `LICENSE-ASSETS`。

## 8. 性能与体积预算（量尺）

| 项 | 目标 | 怎么量 |
|---|---|---|
| 待机 CPU | < 3%（单核口径） | `sample` / `powermetrics` 各 60s；不可见即停后应接近 0 |
| 帧率 | 待机 15fps、说话 30fps、可配 | 窗口内自报计数器 + 日志 |
| 内存 | 先量出来再定上限（WebView 常驻是主要开销，`未实测`） | `ps` RSS |
| 冷启动 | ≤ 1.5s | 从 `gqy pet` 到人物出现 |
| 二进制体积 | pet 的增量 ≤ 3MB（不含模型） | `ls -l` 对比 |
| 模型体积 | 不计入二进制，放磁盘 | `du -sh` |

**没实测数字不合并**（AGENTS §6.1）。

## 9. 施工拆分（每步都能单独验收）

| 步 | 内容 | 动的地方 | 验收 |
|---|---|---|---|
| 1 | 透明置顶小窗 + 静态立绘 + 拖动 + 位置记忆（D 案） | 新 `src/pet/`、`src/bin/pet.rs`、`Cargo.toml`（feature + bin）、层序表 | 桌面上有个能拖的小窗；重启后位置、大小都记得 |
| 2 | IPC 订阅宠物事件流（daemon 侧新增命令 + 事件裁剪） | `src/ipc/protocol.rs`、daemon 侧处理、`src/pet/ipc.rs` | `gqy-pet` 日志里能看到回合开始 / 结束、说话状态变化 |
| 3 | 接 Cubism：示例模型 + 待机 / 眨眼 / 视线 + 说话口型 + 情绪表情 | `src/pet/`、`web/pet/`（页面）、模型放 `state_dir/pet/` | 说话时嘴动、情绪变了表情跟着换；换模型只改配置 |
| 4 | 交互与设置：单击反应、双击开 WebUI、右键菜单、`display.pet` + WebUI 设置卡片 | `src/config/`、`web/settings-schema/`、`src/pet/` | 设置页能开关、缩放；关掉后进程退出 |
| 5 |（可选）包络口型、Linux / Windows 打包、随包发布 | `src/voice/`、IPC 帧、发布链 | 口型跟着音量走；Linux 上能装能跑 |

## 10. 风险与未核实（施工前先打掉的）

1. **wry 的透明窗 + 置顶 + 点击穿透**在目标 macOS 版本上的真实表现：先写 30 行探针验一遍，
   验不过就别谈后面（`未实测`）。
2. **Cubism Core 的许可与再分发条款**：逐条读，尤其「能不能随 Nix 包分发」（`未核实`）。
3. **WebView 常驻内存与电池占用**：笔记本上不可见要停渲染，否则用户会关（§8）。
4. **daemon 重启时宠物怎么办**：daemon 因 `build_id` 变化重启是常态，宠物应自动重连而不是退出
   （照 REPL 的重连口径）。
5. **模型资产与人格的关系**：一个模型还是每个账号一套？v1 先全局一套（§5.3）。
