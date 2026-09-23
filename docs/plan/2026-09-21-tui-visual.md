# TUI 视觉美化施工记录（2026-09-21 起，09-23 续工）

方案稿：`docs/design/2026-09-16-tui-visual-proposal.md`。本文记录施工进度、已拍板的决定和剩余步骤，下次照着续。

**状态**：代码改动都在工作区，未提交、未跑测试。上次 `cargo check --all-targets` 通过是在被中断的那批改动之前，改完之后没有重新检查。分支 `gqy`，基线提交 `2f044939`。

---

## 一、已拍板的决定

| 事项 | 决定 | 来由 |
|---|---|---|
| 三个冲突项 | 空输入占位提示**做**；输入行浅底色带**不做**；回复首行淡色点**不做** | 09-21 用户同意推荐项 |
| 界面主色来源 | **界面色跟终端 16 色槽位走**（竖条、模式标签、选中标记、成败/警告色、淡灰），只把写死的 256 色/真彩（内容色）改成深浅两套 | 用户的 kitty 16 色盘由 matugen 按壁纸生成（`footer.rs` 声波注释，09-05 实录）。方案稿把“16 色随终端主题漂移”当问题，对用户其实是特性 |
| 声波三档色 | 保持原色号不动，加注释说明是有意为之 | 对着壁纸调出来的 |
| 思考过程 | 弱化 + 斜体（`THINKING_STYLE = \x1b[2m\x1b[3m`），不上色 | 方案 P1 |
| 等待动画 | 亮绿 `38;5;10` 改成 `ACCENT`（`34` 槽位）。bright 槽位不碰 | 同上 matugen 理由 |
| 底栏占用条 | `47k/168k ▰▱▱▱▱ 28% · Σ…`：保留数字，占用条替掉括号百分比。窄终端依次：条退成纯百分比 → 丢速度 → 丢累计 → 丢百分比 | 09-23 用户选推荐项 |
| 工具块状态色 | 方案稿的 ✓/◌ 在现有时间线里不存在（每步是工具类型图标）。保持现状：成功 dim、运行中转轮 `ACCENT`、失败整行 `DANGER`，视为已完成 | 09-23 用户选推荐项 |
| 空输入占位提示 | 与 WebUI 同源（人格看板 `composer_placeholder`，没配按人格名生成）。开发模式不显示 | 09-23 用户选推荐项 |

## 二、已完成（工作区）

- **`src/terminal/tone.rs`**（新）：判定终端底色深浅。顺序是 `display.theme` 显式值 > OSC 11 查询（只在 `probe=true` 时，150ms，后跟 DA1 作哨兵）> `COLORFGBG` > 默认深色。另有环境变量 `GQY_THEME` 兜底。`cfg!(test)` 固定深色。带单元测试。
- **`src/terminal/palette.rs`**：`to_256` / `to_16` 改成 `pub(crate)`。
- **`src/render/style.rs`**（重写）：
  - `swatches` 子模块是唯一写 RGB 的地方。`slot("34")` 是界面色，永远发 16 色号，不上色档（Mono）为空。`swatch(dark, light, ansi16)` 是内容色，按 `Tone` 选套、按 `Depth` 降级。
  - `Ansi` 类型是懒求值的具名样式：`format!("{URL_STYLE}…")` 和 `push_str(&X)` 都能用，所以旧常量名全部保留。
  - 新增 `ACCENT` / `ACCENT_DEV` / `MUTED`（纯 SGR 2）/ `SUCCESS` / `WARNING` / `DANGER` / `DANGER_DIM` / `INFO` / `SOFT` / `FAINT`、`mode_accent(dev)`、`expansion_bg()`（返回 ratatui Color）。
  - 测试里固定深底真彩。
- `render/mod.rs` 的 `mod style` 改成 `pub(crate) mod style`，调用方写 `crate::render::style::X`。
- **散落颜色收口**（约 20 个文件）：报错红 → `DANGER`，stderr → `DANGER_DIM`，`36` → `INFO`，`35` → `TERTIARY_STYLE`，`32` → `SUCCESS`，`90` → `FAINT`，patch 的 245/102 → `FAINT`、250 → `SOFT`，思考 `38;5;10` → `THINKING_STYLE`。question_tui 的 `BAR` / `ANSWERED_BAR` 常量改成函数 `bar()` / `answered_bar()`。
- 流式思考（`stream/mod.rs` 的 `write_full_reasoning_chunk` / `print_reasoning`）改为每块自带 `THINKING_STYLE…RESET`。原来是 `SetForegroundColor(Green)`，靠别处 `ResetColor` 收尾，而 `ResetColor` 只复位前景色，斜体会漏到后面的正文上。`tests/reasoning.rs` 已同步。
- `tail/screen/select.rs` 的 `EXPANSION_BG` 常量 → `style::expansion_bg()`。
- 界面色字节与改前一致（`slot` 只发原色号），这部分是纯重构。
- 做过一次审计，所有 `{TOKEN}` 字面量都在格式化宏里。修掉 4 处会把 `{颜色名}` 原样打到屏幕上的 bug。脚本思路：扫描所有含 `{DANGER}` 等的字符串字面量，检查它前面是不是 `format!(` / `write!(x,` 之类。

## 三、续工第一步：先收拾遗留（09-23 已完成）

1. `src/cli/repl/tail/screen/select.rs:76` 留下一段孤儿注释（“展开区的底色。比终端背景深一档……”），删掉或挪到 `style.rs` 的 `EXPANSION_BG` 旁。
2. `cargo check --all-targets`。然后 `cargo fmt`：几处 import 是脚本插入的，比如 `use crate::render::style::{FAINT};` 这种单元素花括号、放置顺序不规范。
3. `style.rs` 顶部模块注释还写着“每个颜色写一次深底 RGB……”，要补上界面色 / 内容色两类的说法（`Swatch` 的注释已经写了）。

## 四、剩余步骤

**09-23 进度**：1–4 已完成（见下），`cargo check --all-targets` 通过、已 fmt。
- 1：`DisplayConfig.theme` + `default_display_theme`；WebUI settings-schema、TUI 设置表各加一项；`web/config_api.rs` 的 `WebDisplayConfig` 是聊天页渲染用的，不需要暴露。`run_repl` 入口 `tone::init(theme, true)`，`run_one_shot` 入口 `init(theme, false)`。`GQY_THEME` 非空时盖过配置。
- 2：`render::format_token_usage_inline_with(meter, percent_fn, speed)`，原 `_opts` 变成它的包装（括号百分比）。底栏在 `footer.rs::context_gauge` / `context_percent`。
- 4：`cli/repl/composer_hint.rs`，文案取 `crate::web::composer_placeholder`；REPL 启动、`reload_repl_config`、远端 REPL 的 /persona 与重载配置处刷新。画在 `render_repl_input_with_footer`，选区用的 `drawn` 记不带提示的那份。


1. **配置项 `display.theme`**（`auto | dark | light`，默认 `auto`）
   - `config/mod.rs` 的 `DisplayConfig` 与 `RawDisplayConfig`，默认值写进 `config/defaults.rs`。
   - `web/settings-schema.js` 的 display 段（`src/web/tests/settings_schema.rs` 会对默认值）；`config_tui/settings.rs` 加一个 choices 字段；看一下 `web/config_api.rs` 是否需要暴露。
   - **接线**：在 `cli/mod.rs` 的 `run_repl` 入口最前面调用 `crate::terminal::tone::init(&config.display.theme, true)`，必须早于任何渲染和输入线程。one-shot 路径调用 `init(setting, false)`，不发查询：shellhook 形态下终端输入不归我们。OOBE 用自己的“夜阑”色板，不管。
   - 属于前后端都要改的功能（AGENTS §8.1）。
2. **底栏占用条** `▰▰▰▱▱ 62%`：< 60% `SUCCESS`，< 85% `WARNING`，其余 `DANGER`。
   - 位置：`footer.rs::repl_footer_line`，右侧目前是整段 `\x1b[2m{right_plain}`，由 `render::format_token_usage_inline_opts` 生成。
   - 窄终端先把占用条退化成纯百分比。现有的丢弃顺序是速度 → 累计 → 百分比，保持不变。
   - `src/cli/tests/footer_tail.rs` 有大量底栏断言。
3. **工具块状态色**：✓ 用 `SUCCESS`，✗ 用 `DANGER`（`timeline.rs:1034` 附近已是 `DANGER`），运行中 ◌ 用 `ACCENT`，块体保持 dim。先读 `render/stream/timeline.rs` 的 glyph 逻辑。
4. **空输入占位提示**（“和顾清影说点什么…”，`MUTED`，一有输入就消失）：WebUI 已有人格可配的占位文案（提交 `a4210a6d`，`persona_identity`），TUI 应复用同一来源。全屏输入框在 `cli/repl/tail/`，inline 在 `cli/repl/layout.rs`。
5. **测试**：`src/cli/tests/` 与 `src/render/tests/` 里约 26 处断言写死了色号。界面色字节没变，大部分应该仍然通过；需要改的主要是思考绿（`38;5;10`）、diff 的 `38;5;245/102/250`、展开区 `Indexed(236)`。改成引用 `style::X`，不要再写字面量。`testkit/tui/run.py:900` 断言 `\x1b[38;5;10m●` 不出现、`:979` 匹配 `\x1b[2m\x1b[36m` 转轮，也要核对。
6. **验收**：`cargo test`、`test_scripts/refactor-check.sh`（涉及 render，属硬要求）、截图对照（见下节），然后写验收流程给用户，通过后提交并更新 `docs/releases/next/release-notes.md`。

## 五、截图对照工具

`testkit/tui/shoot_png.py`：包一层 `testkit/tui/run.py`，run.py 每落一个 `*.txt` 屏，就把 pyte 屏幕渲染成深底、浅底各一张 PNG。pyte 不记录 dim，脚本把 SGR 2 挂到 blink 位上，画成 50% 混色。只依赖 macOS 系统字体，不开窗口。

```sh
python3 -m venv /tmp/tuivenv && /tmp/tuivenv/bin/pip install pyte pillow
cargo build
OUT=/tmp/tui-shots COLORTERM=truecolor TERM=xterm-256color \
  /tmp/tuivenv/bin/python testkit/tui/shoot_png.py after
# 产物：$OUT/png/after/<屏名>-{dark,light}.png
```

- 改前基线：在 `2f044939` 上以标签 `before` 跑一遍。上次的基线存在会话临时目录里，已不可靠，需要重跑。
- 基线上 run.py 有 3 项**本来就红**：`input_bar_at_bottom`、`step_expands`、`item01 空回车不发消息`。与本次改动无关，对照时别误判。
- 基线实测的浅底问题：展开区 `Indexed(236)` 深灰底 + diff 深色底，在浅底上看不清。这正是本次要修的。
- 注意：这里的“浅底”是渲染脚本把默认底色画成白色的模拟，程序本身仍按深色输出。想看真实浅色效果，要带 `GQY_THEME=light` 跑。
