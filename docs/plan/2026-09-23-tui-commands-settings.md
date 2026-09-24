# TUI 斜杠命令选择与设置补全（2026-09-23；第一节 09-24 已落地）

来由：用户提出「输入斜杠时 TUI 会预览命令，但不能用方向键选择；原有命令不足以完整展示 gqy 的设置，建议都加，必要时可以调整文件树结构」。09-23 调研完成、方案经用户同意，但为了不干扰同期的 CHANGELOG/发版改造，本文只记方案，不施工。

---

## 一、方向键选择斜杠命令

### 现状与根因

- 候选来自 `slash_commands::repl_command_suggestions`（前缀过滤 `REPL_COMMAND_TABLE`）。
- 全屏：`cli/repl/commands.rs::command_hint_lines` 在输入框上方画浮层，最多 4 条「命令 + 说明」，由 `tail/frame.rs` 每帧调用。
- inline：`cli/repl/input.rs::render_repl_input_with_footer` 在 footer 位置挤一行命令名（`repl_command_suggestions_line`）。
- 按键：`cli/repl/editor.rs`（live 编辑器，全屏与 inline 共用）的 `KeyCode::Up/Down` 固定翻输入历史；`KeyCode::Tab` 只在候选唯一时补全（`complete_repl_command`）。直连模式另有一套旧循环 `input.rs::read_repl_input`，按键处理是复制的一份。
- **根因**：候选只是渲染产物，编辑器里没有「选中第几条」的状态，方向键无处可去。
- 这条交互 09-10 就已定稿（`docs/plan/2026-09-10-tui-rewrite.md` §4：「输入 `/`：命令列表锚在输入框上方，↑↓ 选、Tab 补全、Enter 执行、Esc 清空」），是施工时漏掉的，不是新需求。注意定稿写的是 Enter **执行**，与下面方案第 2 条的「只填入不执行」不同，开工前与用户确认。

### 方案

1. 编辑器状态加 `command_pick: Option<usize>`（选中下标）。输入变化时：候选为空置 `None`，否则夹到新长度内（默认 0）。
2. 有候选时：
   - Up/Down 在候选里移动，首尾循环；**不再翻历史**。
   - Tab 或 Enter 把选中项填进输入框（带参数的命令补一个空格，光标到末尾），不直接执行。候选唯一且已完全匹配时 Enter 照常提交。
   - Esc 关掉候选（复用全屏已有的 `command_hint_dismissed`），关掉后方向键回到翻历史。
3. 无候选时行为不变。
4. 渲染：全屏浮层选中行用 `ACCENT` 前景 + `▸` 标记，其余保持现样。浮层超过 4 条时跟着选中项滚动窗口。inline 那一行把选中项用 `ACCENT` 标出。
5. 两套按键循环都要改。**先把选择逻辑抽成纯函数**（输入：候选列表、当前下标、按键；输出：新下标 / 填入文本），放 `cli/repl/commands.rs`，`editor.rs` 与 `read_repl_input` 各调一次，避免再复制一份。

### 施工记录（09-24 已落地）

按 09-10 的最终裁定落地，和上面第 2 条有两处不同：

- **Enter 直接执行选中的命令**（不是只填入）。必填参数的命令（`arg_hint` 以 `<` 开头，如 `/rename <name>`）没法空着执行，Enter 改为填入加空格。默认选中和输入完全相同的那条，所以打全了命令直接回车，执行的仍是打的那条。
- **Esc 关掉后方向键回到原语义**：输入框里有字时 ↑↓ 是挪光标（翻历史只在空框或历史原样时发生，这是原有规则）。

实现是 `cli/repl/command_picker.rs`：状态挂在「当时那一串输入」上（`anchor`），输入一变选择与「已关掉」自然作废，不用在每个改输入的地方挂钩子。全屏原来在 `Screen` 里的 `hint_dismissed` 并进来，单一来源。`editor.rs` 与直连模式的 `read_repl_input` 共用它。候选只在输入不含空白时出现，第二节的「参数候选」在这里扩展。

### 测试

- 纯函数单测：循环、夹紧、唯一候选 Enter 直接提交、Esc 后方向键回历史。
- `src/cli/tests/` 里补全屏帧断言：打 `/s` 后按 Down，浮层第二行带选中标记。
- `testkit/tui/run.py` 加一项：`/` → Down → Enter，输入框内容是第二条命令。

## 二、设置补全

### 现状

`/config` 打开的终端配置器（`src/config_tui/`）与 WebUI 设置页（`web/settings-schema.js`，147 个字段）对照，按字段路径粗扫，终端缺约 31 项：

| 分组 | WebUI 字段数 | 终端缺 | 缺的是什么 |
|---|---|---|---|
| context | 15 | 13 | 默认窗口、工具输出落盘阈值、compact 缓存复用与文件恢复等 |
| cache | 5 | 5 | 请求日志、保留天数、keepalive、写入宽限 |
| tools | 9 | 4 | 子代理并发、默认超时、命令黑名单、沙盒可写目录 |
| notifications | 3 | 2 | 回合完成通知、后台任务写回终端 |
| accounts | 2 | 2 | 成员人格、成员插件 |
| plugins.memory | — | 5 | 联想条目长度、片段长度、遗忘天数、学习阈值 |

斜杠命令侧只有 `/config` 一个入口，进去是完整菜单，不能直达某一组。

### 方案

1. **文件树**：`config_tui/settings.rs`（「全局参数设置」一张大表单）拆成目录：

   ```
   config_tui/settings/
     mod.rs            子页菜单 + 分组注册表
     display.rs        显示（现有字段 + theme）
     context.rs        上下文
     tools.rs          工具
     cache.rs          缓存
     notifications.rs  通知
     accounts.rs       账号
   ```

   分组 id 与 WebUI `settings-schema.js` 的 section id 一致（`display`/`context`/`tools`/`cache`/`notifications`/`accounts`），记忆插件参数并入现有 `plugin_settings.rs`。
2. **直达**：`/config <分组>` 直接打开对应子页。`REPL_COMMAND_TABLE` 的 `/config` 的 `arg_hint` 写成 `[display|context|tools|cache|notifications|accounts]`，补全面板在打出 `/config ` 之后列出分组名（需要给补全加「参数候选」这一层，和第一节的选择状态共用）。
3. **防再漏**：加一条测试，读 `web/settings-schema.js`（复用 `src/web/tests/settings_schema.rs` 的解析器），断言每个字段路径在终端配置器的字段注册表里都有，或列在显式豁免清单里（例如只在 WebUI 有意义的 QQ 平台细项）。这需要终端字段带上配置路径，顺手把 `Field::new(...)` 的绑定改成按路径声明。
4. 前后端对照（AGENTS §8.1）：本项只补终端一侧，WebUI 已全。

### 影响面

- `settings.rs` 现在约 280 行，拆后每个文件都在 200 行内。
- `config_tui` 是入口层，不涉及跨层引用。
- 不改配置结构、不涉及迁移。

## 三、施工顺序建议

先做第一节（体量小、用户每天都碰），再做第二节。两节共用「补全候选 + 选择状态」，第一节把它做成能挂参数候选的形状，第二节直接用。
