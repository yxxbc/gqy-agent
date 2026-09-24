# TUI 启动行为与开屏欢迎框（2026-09-23 方案，09-24 已施工）

**09-24 施工记录**：§二（`49e55909`）与 §三 已实施。代码在 `cli/repl/banner/`：`mod.rs`（对外接口不变：load / lobby / render_ansi…，`tick()` 恒为 false，开屏静止）、`welcome.rs`（欢迎框：并排或叠放，最近会话，相对时间）、`mascot.rs`（立绘 kitty / 半格 32 与 24 列、黑猫、自定义、关）。立绘素材 `assets/mascot/portrait.png` 由 `testkit/tui/mascot.py --export-portrait` 生成（256×261，136 KB）。与方案的偏差：欢迎框宽度取终端 3/4、最多 84 列（方案未定具体值）；并排时右栏最少 34 列（模式行整行不截）；「提示行轮换」简化为按有无历史会话二选一；Tab 换车道后「最近」仍是启动时那条车道的（未重读）。沙箱 100×40 截图核对：立绘、四行字、同宽输入框、底栏都在位。

来由：用户提出两件事。一是「TUI 打开后默认进上次关闭的对话，改成 `-c` 才进」；二是「开屏要不要改成 Claude Code 那种有吉祥物的」。09-23 拍板如下，与 `2026-09-23-tui-input-box.md`（圆角输入框）排成同一批施工、一起验收：开屏、输入框、会话默认行为是同一片区域，分开做会反复改同一块代码。

---

## 一、已拍板

| 事项 | 决定 |
|---|---|
| `gqy` / `gqy dev` | 每次打开是新会话，因此每次都先看到开屏 |
| `gqy -c` / `gqy dev -c` | 回到这条车道上次的会话（即现在的默认行为） |
| `gqy --session <名字>` | 顺带放开：直接打开指定会话（现在对 REPL 报错） |
| 上次那条是空会话 | **复用**，不新建。反复开关不攒空会话 |
| 开屏样式 | **Claude Code 式欢迎框**：小框里左边吉祥物、右边欢迎语与模型/目录、一行提示；框下列最近会话。大号 GQY 艺术字与星空扫光撤掉 |
| 吉祥物素材 | 用户 09-23 提供了像素风立绘，存为 `assets/mascot/gqy-mascot-source.png`（1254×1254 透明底，AI 生成，只作源文件，不打包） |
| 吉祥物显示 | kitty 贴原图（约 16 列 × 8 行）。其他终端用半格字符画：高度够用 32 列 × 17 行，不够退 24 列 × 12 行，再不够不画 |
| 吉祥物裁剪 | 头肩（到领口），脸最大。转换工具 `testkit/tui/mascot.py`。09-23 用户看过 32 列字符画真实效果，满意 |
| 吉祥物可选（09-24） | 新设置项 `display.mascot`：`portrait`（顾清影立绘，默认）/ `cat`（黑猫字符画）/ `custom`（读 `config/banner.txt`）/ `off`。WebUI 设置页与 `/config` 都能改 |
| 黑猫字符画 | 用户提供，存为 `assets/mascot/cat.txt`（23 行 × 48 列，纯 ASCII）。青瓷绿渐变上色，深浅底各一套；16 色终端退纯青色（`36` 槽位），黑白终端用默认前景色 |
| 自动降级 | 选 `portrait` 但终端画不出（16 色 / 黑白）时自动换黑猫；黑猫也放不下就不画 |
| `config/banner.txt` | 保留，改为 `custom` 选项的来源（原「开工前确认」的悬案就此解决，不进 CHANGELOG 的 Removed） |
| 排期 | 先成文档，与圆角输入框同批做 |

## 二、启动行为

### 现状

- 远端 REPL（`cli/repl/remote/interactive.rs:27`）发 `IpcCommand::GetReplSession { mode }`，daemon 侧（`web/ipc_server.rs:204`）调 `StateStore::ensure_repl_session`（`state/sessions.rs:202`）：车道指针有效就返回它，没有才 `new_repl_session`。
- 直连 REPL（`cli/repl/direct.rs:328`）直接调同一个 `ensure_repl_session`。
- `-c/--continue`（`cli/args.rs:160`）目前只给单次命令；裸 REPL 带 `-c` 或 `--session` 在 `cli/mod.rs` 里直接 `bail!`。
- `gqy dev` 是子命令，接不到根上的 `-c`。

### 改法

1. `StateStore` 加 `fresh_repl_session(persona)`：取车道指针，指向的会话**没有任何回合**就原样返回（复用空会话），否则 `new_repl_session`。`ensure_repl_session` 保留给 `-c`。
2. `IpcCommand::GetReplSession` 加字段 `resume: bool`（`#[serde(default)]`，缺省 false 即新开）。CLI 与 daemon 同版本由 `GQY_BUILD_ID` 保证，旧字段兼容只是保险。
3. `run_repl(paths, mode, launch)`，`launch` 是 `Fresh | Resume | Session(String)`。`cli/mod.rs` 里去掉对 REPL 的 `-c` / `--session` 报错，改为传进来。`Command::Dev` 加 `#[arg(short = 'c', long = "continue")]`。
4. `display.repl_replay_turns`（重开回放几轮）只在 `Resume` / `Session` 时生效，配置说明同步改。WebUI 设置页的 hint 同步（`web/settings-schema.js`）。
5. 帮助文本、`docs/wiki/03-使用方式.md` 与 `04-命令参考.md` 同步。README「终端」一节加一句 `gqy -c`。

### 测试

- `state/tests/sessions.rs`：空会话复用、非空会话新建、`ensure_repl_session` 行为不变。
- `tests/daemon_reload.rs` 与 `src/cli/tests/` 里断言「重开回到原会话」的用例改走 `-c`，新增「裸打开是新会话」。
- 黑盒（GQY_HOME 沙箱，AGENTS §5.4）：打开说一句、退出、再打开应是空会话；`-c` 回到刚才那句。

## 三、开屏欢迎框

### 现状

`src/cli/repl/banner/`（约 660 行）：渐变 GQY 艺术字（可被 `config/banner.txt` 替换）、两侧星空、周期扫光、模式行。只在空会话出现，全屏铺成「大厅」、inline 塞在输入框上方。`display.banner = false` 整个关掉。

### 目标画面

```
╭──────────────────────────────────────────╮
│  <吉祥物>   欢迎回来，顾清影在这儿          │
│  <约 8 行>  deepseek-v4 · ~/proj            │
│             普通模式 · Tab 切换开发模式      │
│             提示：gqy -c 回到上次 · /help  │
╰──────────────────────────────────────────╯

  最近：修 CI 报错（2 小时前）· 周末去哪（昨天）· …

╭──────────────────────────────────────────╮
│ ❯ 给 顾清影 发消息                          │
╰──────────────────────────────────────────╯
  普通 · deepseek-v4 · opencode    0/168k ▱▱▱▱▱ 0%
```

- 欢迎框框线 `FAINT`，与圆角输入框同一套画框函数（施工时先写输入框，再复用给欢迎框）。
- 欢迎语：有 `-c` 可回的上次会话时说「欢迎回来」，全新安装说「初次见面」。人格名取当前人格（与空输入提示同源）。
- 「最近」：当前车道最近 3 个**非空**会话的标题与相对时间。标题为空的跳过。没有就整行不画。
- 模式行（原来画在艺术字下）并进框里第三行。
- 提示行按情形轮换一条（`gqy -c`、`/session`、`/help`、`Tab` 切模式），不做动画。
- 窄终端：宽度不够放吉祥物时只画右半部分文字；再窄只画一行欢迎语。

### 吉祥物

- **素材**：`assets/mascot/gqy-mascot-source.png`，用户提供的像素风顾清影立绘：黑长发、白梅与青叶发饰、珍珠流苏、白纱衣、托腮。
- **转换工具**：`testkit/tui/mascot.py`。裁头肩（到领口）、对比度 1.1、锐化 1.4、BOX 缩放，输出 `.ansi`（可直接 `cat` 进终端看）和深/浅底并排预览。09-23 试过的结论：
  - 整幅半身像，16 列糊成一片，24 列勉强，32 列才看得清。
  - 裁成头肩后，同样宽度脸大一倍：24 列认得出，32 列眼睛、腮红、托腮的手、头花都清楚。
  - 只裁到嘴（58%）会切掉下巴，而且对比度 1.25 会把脸冲成死白。
- **kitty**：用项目已有的 kitty 图片协议贴原图，占约 16 列 × 8 行。施工时从源图生成一张约 256px 的小图，`include_bytes!` 编译进二进制（源图 1.4MB，不直接嵌）。
- **其他终端**：施工时由 `mascot.py` 生成 32 与 24 两档，产物是每格「字符 + 前景色 + 背景色」的文本，编译进二进制（与现有艺术字同路）。按终端剩余高度选档，都放不下就只画文字。
- **配色**：字符画按原图真彩出。256 色终端走现有的 `to_256` 降级；`Depth::Ansi16` 与 `Depth::Mono` 不画吉祥物（16 色画不出这张图）。深浅底都已预览过，发色与白衣在两种底上都有轮廓，不需要额外描边。
- **署名**：素材是 AI 生成的形象，施工时在 README 致谢或 `assets/mascot/` 下的说明里注明来源。

### 吉祥物选项 `display.mascot`

| 值 | 画什么 | 尺寸 | 画不出时 |
|---|---|---|---|
| `portrait`（默认） | 顾清影立绘：kitty 贴图，其他终端半格字符画 | 16×8 / 32×17 / 24×12 | 16 色或黑白终端换 `cat` |
| `cat` | 黑猫 ASCII 字符画 `assets/mascot/cat.txt`，青瓷绿渐变 | 48×23 | 放不下就不画 |
| `custom` | `config/banner.txt` 原样画（沿用现有 `BannerArt::from_text` 的读取与宽度检查） | 按文件 | 文件缺失或放不下就退回 `portrait` 的规则 |
| `off` | 不画吉祥物，欢迎框只有文字 | — | — |

- **黑猫的高度**：23 行，放进欢迎框后开屏约需 30 行。低于这个高度的终端不画黑猫，只留文字，不做缩放（ASCII 字符画缩不了）。
- **青瓷绿渐变**：按行从淡灰（顶）过渡到青瓷绿（底），与立绘的青叶、黑猫的翡翠流苏同一色系。它是内容色，照 `render/style.rs` 的 swatch 写法各备深浅两套；`Depth::Ansi16` 用 `36` 槽位，`Depth::Mono` 不上色。
- **配置落点**：`DisplayConfig` 加 `mascot`（默认 `portrait`），`web/settings-schema.js` 与 `config_tui` 的显示分组各加一项，前后端同步（AGENTS §8.1）。`display.banner = false` 仍然整个关掉欢迎框，优先于 `mascot`。

### 撤掉什么、保留什么

- 撤：大号渐变艺术字、星空、扫光动画。空闲时开屏不再每 40–80ms 重绘，空闲 0 重绘的预算（`2026-09-10-tui-rewrite.md` §1）反而更好守。
- `config/banner.txt`：保留，成为 `display.mascot = custom` 的来源（09-24 定）。
- 保留：`display.banner = false` 仍然整个关掉欢迎框。

### 推翻的定稿

同圆角输入框：`2026-09-10-tui-rewrite.md` §3「无边框盒子（通知是全局唯一允许的边框）」。施工时在该条后注明被本文与 `2026-09-23-tui-input-box.md` 取代。

### 测试

- 欢迎框每一行显示宽度等于框宽（含中文人格名、长模型名、长路径截断）。
- 「最近」只列非空会话、最多 3 个、无则不画。
- 窄终端三档退化。
- `Depth::Mono` 不画立绘，`portrait` 自动换成不上色的黑猫。
- `display.mascot` 四个值各一条：画出来的是对的那一幅；`custom` 文件缺失时退回 `portrait`。
- 终端矮于黑猫所需高度时不画黑猫。
- 截图对照：`testkit/tui/shoot_png.py` 深浅底各一套。

## 四、这一批的施工顺序

1. 启动行为（`-c`），独立、可先验收。
2. 圆角输入框（`2026-09-23-tui-input-box.md`）。
3. 欢迎框，复用第 2 步的画框函数。吉祥物素材已到位。

## 五、验收要点（给用户照做）

1. 打开 `gqy` 说一句话，退出，再打开：是新的空会话，看到欢迎框，「最近」里有刚才那条。
2. 什么都不说就退出，再打开：没有多出一条空会话（`/session` 里看）。
3. `gqy -c`：回到刚才那条，回放最近几轮。`gqy dev -c` 同理回开发车道。
4. 欢迎框里的人格名、模型、目录都对。拖窄窗口，框按三档退化，不错位。
5. 浅色终端下吉祥物看得清。
6. `/config` 里把吉祥物换成黑猫，下次开屏是青瓷绿的黑猫；换成「关」只剩文字。
