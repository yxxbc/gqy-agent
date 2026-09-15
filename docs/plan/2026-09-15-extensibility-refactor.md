# 可扩展性重构施工记录（2026-09-15）

依据：`docs/code-review-2026-09-15.md`。评审里的**分析**逐条核查后采用；评审给的**修改方案**不照搬，
每一项按核查结果重新设计。本文记录核查结论、实际做法、刻意不做的事，以及验收步骤。

---

## 一、核查结论（评审说的对不对）

| 评审论断 | 核查 | 备注 |
|---|---|---|
| 描述 JSON 靠手写 `include_str!` 清单，漏行静默降级 | **属实** | 46/46 手工对齐；`apply_built_in_description` 找不到 JSON 时无日志 |
| `plugin_label` 兜底空串，album/map/express 无标签 | **属实** | 被 `TOGGLE_PLUGINS` 过滤暂时不可见 |
| `compose_registry` 手写 if 链，无表 | **属实** | |
| 「加插件碰 13 处」 | **夸大** | 13 处里只有 `PLUGIN_IDS`、`plugin_label`、`TOGGLE_PLUGINS`、compose 块是同一概念的重复清单；`config_tui/plugins.rs`（机器级开关，id 空间不同）、descriptions 分组（工具组）、`paths/resources.rs`（资源目录）、前端面板是不同概念 |
| `stream.rs:26-32` 空 if 块是死代码 | **属实** | 追到 `3c3c8562`：advisory 重复提醒 `repeat_chain` 整体退役时删了块体、留了条件与注释，不是误删行为 |
| `AGENTS.md §5.5` 脚本路径过时、`refactor-check.sh` fmt 注释过时 | **属实** | 另发现 `arch_dep_check.py`、`refactor_size_report.py` 用法说明、wiki 14 同样过时；仓库 fmt-clean，渐进 fmt 门禁已无意义 |
| 依赖门禁只管 8 条边 | **属实** | 实测跨模块边 237 条 |
| `map_turn_row` 位置索引 + 列清单复制 7 遍 | **属实** | |
| `settings-schema.js` 默认值无比对 | **属实** | 且设置页拿到的是 `to_value(AppConfig)`，等于默认值的分区（platforms/embedding/cache）会被省略，页面此时**用的就是 schema 默认值**——漂移影响比评审说的大 |
| TUI 位置索引（settings.rs `debug_assert` / QQ 菜单魔法数） | **属实** | `plugins.rs` 同一家族更严重：下标在 4 个函数间对齐，另有「最后一项 = api_quota」隐式约定 |
| 可读名表 68 条与 JSON `display_name` 职责重叠 | **部分属实** | 79 条里 34 条是历史名/非 JSON 工具的别名；覆盖性已有测试；5 个中文名有出入（外观问题） |
| `AgentMode` 336 处 / 两个真相源 | **部分属实** | `Dev ⇔ dev persona` 单向映射；但 mode 还决定 dev 提示词源、`<mode-update>` 注入、claude-code 中转作用域等，**直接影响提示词字节与缓存** |

---

## 二、已完成

### 1. 描述 JSON 自动登记
- `build.rs` 扫 `src/tools/descriptions/*.json` 生成 `TOOL_DESCRIPTION_FILES`，手写宏删除。重名 JSON 启动即 panic 指出文件。
- 反方向守护：`shape_tests::registered_built_in_tools_have_json_descriptions`——注册了却没 JSON 的工具（除 4 个刻意 Rust-only 的）报红。先于夹具刷新拦截，不会把错误状态钉进基线。

### 2. 插件单一目录 + 声明式工具装配
- `config/plugin_catalog.rs`：`PLUGINS` 一行写全 id/显示名/说明/引导开关；`PLUGIN_IDS`、`TOGGLE_PLUGINS` 编译期派生（顺序不变，persona.toml 落盘字节不变）；album/map/express 补了名字。
- `tools/compose.rs`：`compose_registry` 改为有序注册单元表 `UNITS`（顺序与旧 if 链逐项一致，`shape_tests` 夹具证明三个面 tools 数组逐字节不变）。表与目录一一对应由 `plugin_units_match_the_plugin_catalog` 钉着。
- 顺带：`tools/mod.rs` 1502 → 1220 行，回到上限内。
- 为什么不把注册函数挂在 config 的目录上：config 在层序上低于 tools，不能反向依赖。

### 3. 数据库行映射按列名
- `rows.rs` 新增 `TURN_COLUMNS`；`history.rs` 7 处列清单改用它；`map_turn_row` 22 个位置索引改列名读取。

### 4. TUI 位置索引家族
- `widgets/form.rs` 新增 `BoundFields`：字段与写回写在一起，读回不按下标。
- `settings.rs` 全局设置、`plugins.rs` 全部插件表单改用它；`plugins.rs` 用 `TuiPlugin` 枚举取代菜单下标（漏分支编译不过）。
- `platforms/mod.rs` QQ 菜单改 `QqRow` 枚举分发，`23 - usize::from(!parallel)` 魔法数消失。

### 5. 设置页 schema 同步测试
- `web/tests/settings_schema.rs`：读 `settings-schema.js`（不执行 JS，CI 无 node）→ 对 general/toolPlugins/qq 约 200 个字段，经 serde 往返探测「键是否存在」与「默认值是否一致」。`qqPlugins` 是各平台插件自解析的自由 JSON，无 Rust 默认值，不查。
- 当前零漂移。

### 6. 依赖门禁改层序表
- `arch_dep_check.py`：8 层层序表全对比较，新顶层模块不归层直接失败；`--tighten` 只降不升收紧白名单。
- 白名单从 8 条边扩到 18 条（新增 10 条是原盲区里的既有反向依赖，冻结只减不增）。

### 7. 死代码与文档
- 删 `stream.rs` 空 if 块；AGENTS.md §2.1/§3.1/§5.5、wiki 14、两个门禁脚本用法同步；`refactor-check.sh` 格式步改全仓 `cargo fmt --check`，删除过时的 `fmt_no_regress.py`。

### 守护有效性（AGENTS §5.1：先证明会红）
逐个做了变异验证，全部报红后复原：
- schema 默认值 180→181、路径拼错 → schema 测试红
- compose 表去掉 album 的插件 id → 一一对应测试红
- 挪走 `alarm.json` → 描述守护测试 + 形状夹具同时红
- 在 `src/llm/` 放一条 `use crate::web` → 层序门禁红

---

## 三、刻意不做（需要用户拍板）

| 项 | 为什么没做 |
|---|---|
| `AgentMode` 退场 | mode 分支直接决定系统提示词字节（dev 提示词源、`<mode-update active=…>`、风格锁、预设对话），改动=计划外全量冷启动，且 IPC/WebUI DTO 带 mode 字段。收益（去掉一个与 persona 单向映射的枚举）不抵风险，应单独排期、带两轮 cache-usage 手测 |
| 子系统挂接点抽 trait | 挂接点 A/C 今天只有记忆一个实现者，抽 trait 是为单一实现造接口；等出现第二个子系统再抽 |
| `Agent` 43 字段拆分 | 纯搬家，触及回合热路径与大量测试，收益主要是审美；无实测数字支撑（AGENTS §6.1） |
| 拆 `web/app.js` | 13k 行单 IIFE、509 个顶层声明共享闭包；拆成多个经典脚本会把函数提升语义与全局命名暴露出来，无浏览器端自动化测试兜底，无人值守施工风险过高 |
| 可读名表收口到 JSON | 见核查表：大半是历史别名，覆盖性已有测试守着，合并收益小 |

---

## 四、施工中发现的既有问题

- **已修**：`refactor-check.sh` 在测试全绿之后必定失败——`echo "用例数 $now（基线 $before）"` 里
  `$now` 紧跟全角括号，中文 locale 下 bash 把它当成变量名的一部分，`set -u` 报「未绑定的变量」。
  也就是说这道门禁此前从来跑不到「模型面语言 / 文件规模 / 依赖方向」三步。两处都改成 `${now}`。
  最终结果：全部门禁通过（fmt、编译、2363 个用例 0 失败、模型面英文、文件规模、依赖方向）。
- **未修**：`platforms::onebot::tests::notices::bot_send_availability_queries_self_once_and_uses_the_cache` 偶发失败：
  与同文件其他用例共享进程级 `group_mute_cache()` 且 self_id 相同，并行时别的用例 `remove_account`
  会在两次查询之间清掉缓存。单跑 15/15、模块跑 5/5 通过，全量并行时偶发。与本次改动无关。

---

## 五、验收流程

1. `bash test_scripts/refactor-check.sh` —— 全绿（若只挂上面那条 onebot 偶发用例，重跑一次）。
2. `python3 test_scripts/arch_dep_check.py` —— 看到按层标注的 18 条边、门禁通过。
3. 终端配置器（`GQY_HOME` 沙箱）：
   - 「全局设置」改任一项保存 → 重开确认只有那一项变了；
   - 「插件」空格开关每个插件、回车改表情包「自动提示发送表情概率」→ 保存后重开确认；
   - 「腾讯 QQ」关掉「会话内并行」→ 光标下移到「私聊/群聊专属配置」「QQ 插件配置」「高级设置」，回车分别进对应界面（旧魔法数最易错的三项）。
4. WebUI 成员引导「自选功能」列表显示正常（插件名来自新目录）。
5. 两轮对话后看 `cache-usage.*.jsonl`：第二轮 `cache_read` 正常（工具面按夹具证明未变，这一步是兜底）。
