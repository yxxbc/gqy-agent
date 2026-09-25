# 外部扩展进设置页的插件列表（2026-09-24 方案，09-26 已施工）

来由：用户问「当前的插件是不是只能用内置的，不能注册」。答案是内置插件（Rust 编译进二进制）确实不能在外部注册，但脚本工具、MCP 服务器、技能、`gqy pm` 包都能不改代码地加。用户在三个方向里选了「让外部扩展出现在设置的插件列表里」，先写方案，不施工。

同一次对话里顺手修掉了 `gqy pm` 写死的失效默认索引（`SHORiN-KiWATA/gqy-packages` 根本不存在），与本方案无关。

---

## 一、现状

| 扩展 | 怎么加 | 在哪管 | 开关 |
|---|---|---|---|
| 内置插件 | 改源码（`config/plugin_catalog.rs` + `tools/compose.rs` 的 `UNITS`） | 设置 →「插件」页，数据来自写死的 `web/settings-schema.js` 的 `toolPlugins` | 卡片上的开关 |
| 脚本工具 | `manage_script` 注册，或 `gqy pm` 装包 | 控制台「脚本」面板（`web/dash-scripts.js`，`/api/dash/scripts/*`） | 面板里禁用 / 启用 / 删除 |
| MCP 服务器 | 配置 `mcp.servers` | 设置页「MCP」段 | 每个服务器的 `enabled` |
| 技能 | `manage_skill` 创建，或手放 `extensions/skills/`，或 `gqy pm` | 设置页「技能」段只有总开关 `skills.enabled` | 只有总开关；逐个开关只在人格清单里（见下） |
| pm 包 | `gqy pm install` | 只有命令行 `gqy pm list / remove / upgrade` | 无，WebUI 看不到 |

按人格的逐项白名单已经存在：`PersonaManifest` 的 `plugins.scripts` / `plugins.skills` / `plugins.mcp`（`config/persona_manifest.rs`），`None` = 全部可见。引导「自选功能」那一屏与 WebUI 成员引导都用它（`config/feature_catalog.rs`）。

问题：同样是「给她加能力」，内置插件在「插件」页，其余四种散在三个地方，pm 包在 WebUI 里完全看不到。用户想加了什么、开着什么，要翻好几处。

## 二、目标

设置 →「插件」页在内置插件分组之后加一个「扩展」分组，把四类外部扩展用同样的卡片摆出来：

- **卡片**：名字、一句话说明、来源标签（`脚本` / `MCP` / `技能` / `pm 包`），能开关的带开关。
- **点开抽屉**：详细信息与操作。
  - 脚本：参数说明、来源文件，操作是禁用 / 启用 / 删除，与「脚本」面板同一套后端。
  - MCP：命令与参数、提供了哪些工具、连接状态，操作是启用 / 停用，改配置跳到设置的 MCP 段。
  - 技能：说明、文件位置，操作是启用 / 停用。
  - pm 包：版本、来源仓库与 commit、装进来的文件清单，操作是升级 / 卸载。
- **分组内按来源再分小节**，每节末尾一个「添加」入口：脚本和技能写明「让她用 manage_script / manage_skill 创建」，MCP 跳到配置段，pm 包给安装框（输入 `owner/repo`）。

「脚本」控制台面板、MCP / 技能设置段都保留，插件页是汇总入口，不替代它们。

## 三、要补的后端

1. **聚合接口** `GET /api/extensions`：一次返回四类扩展的列表与开关状态。每条 `{kind, id, title, description, source, enabled, toggleable, package}`，`package` 是它属于哪个 pm 包（从 `extensions/pm/lock.json` 反查文件归属），不属于就为空。
2. **技能逐个开关**：现在只有人格白名单能逐个关技能，管理员没有全局的逐个开关。两种做法，**施工前与用户确认**：
   - 推荐：开关写进当前人格的 `PersonaManifest.plugins.skills` 白名单，和成员引导、自选功能一个机制，不新增配置项。
   - 另一种：新增全局 `skills.disabled` 名单。
3. **pm 包的 WebUI 操作**：`POST /api/extensions/packages/{name}/upgrade`、`DELETE /api/extensions/packages/{name}`，复用 `src/pm/` 现有的 upgrade / remove。安装入口是否在 WebUI 开放（要确认文件清单再装，涉及下载外部代码）**施工前与用户确认**，推荐先只做升级和卸载，安装仍走命令行。
4. 写操作都走 `require_admin_mutation`，成员看不到这个分组（成员有自己的「账号 → 人格」勾选）。

## 四、前后端对照（AGENTS §8.1）

- **WebUI**：`settings.js` 的 `renderPluginsPage` 加「扩展」分组与抽屉。卡片图标：脚本、MCP、技能、包各一个通用图标，不用首字母。
- **TUI**：`config_tui/plugins.rs` 的插件菜单末尾加一项「扩展」，进去是四类列表，能开关的给开关，其余只读显示。pm 的升级 / 卸载在 TUI 里提示用命令行。**TUI 做到什么程度施工前确认**，推荐只读 + 开关。

## 五、测试

- `/api/extensions`：四类各造一个，断言返回齐全、`package` 归属正确（pm 装的脚本能反查到包名，手工注册的为空）。
- 开关：脚本禁用后不出现在工具面；MCP 停用后不拉 tools/list；技能按选定方案生效。
- pm 卸载经 WebUI 触发后，lock.json 与文件都清理，和命令行 `gqy pm remove` 结果一致。
- 截图：插件页「扩展」分组，深浅主题各一张。

## 六、验收要点（给用户照做）

1. 设置 → 插件，最下面有「扩展」分组，能看到你的脚本、MCP 服务器、技能和 pm 装的包。
2. 关掉一个脚本，下一轮对话里她不再用它；再打开恢复。
3. 点开一个 pm 包，看到版本和文件清单，能升级和卸载。
4. 用成员账号登录，看不到这个分组。

---

## 七、施工记录（09-26）

施工前与用户定下的几处：

- **范围**：四类一起做。技能逐个开关写进当前人格清单的 `plugins.skills` 白名单；WebUI 能用表单新建技能；pm 只做升级 / 卸载，安装仍走命令行；TUI 只读加开关。
- **技能开关分三种**（`src/skills/admin.rs`）：平台级内置技能常开不给关（`fixed`）；人格自己那一层的技能不受白名单管，用目录里的 `.disabled` 标记（`marker`，与 `gqy skills disable` 同一机制）——否则她用 manage_skill 新建的技能会默认看不见；全局层与可选内置技能写白名单（`whitelist`），白名单原为 None 时第一次关某个会展开成「当前开着的全部」。
- **来源分两种**（用户 09-26 追加）：互联网来的（git 克隆的 MCP 服务器与技能目录、pm 包）与自己创建的。`src/pm/origin.rs` 按路径向上找 git 仓库（仓库根不能是 gqy 数据目录的上级，防家目录 dotfiles 仓库误判），记远端、commit 和依赖同步方式（npm / pnpm / yarn、uv / pip）。「检查更新」抓远端默认分支（有上游用上游）；「更新」在已跟踪文件有本地修改时拒绝，分支上快进、分离头指针就切到新提交，再同步依赖。只接受当前识别出来的扩展目录。由别的程序管理的（不在 gqy 目录里也不是 git 仓库）只显示不更新。
- **她写的 6 个脚本搬进源码**（`src/scripts/personas/default/`：afu_scale、anysearch、blender_model、iching_divination、macos_reminders、macos_news）。脚本头新增 `Platform:`（`macos` / `linux`），不匹配当前系统的在扫描时就跳过；两个 macOS 专用脚本标了它，blender_model 改成先找 `BLENDER_BIN`、PATH 再回落 macOS 路径。装上新版后删掉 `~/.gqy/extensions/scripts/` 里的旧副本。
- 改完不用重载 daemon：技能与脚本目录每一轮按指纹重扫。

代码位置：`src/web/extensions_api.rs`（聚合与写操作）、`web/settings-extensions.js`（「扩展」分组，零件从 settings.js 借）、`src/config_tui/extensions.rs`（插件菜单末尾「扩展」）、`src/pm/upgrade.rs`（升级拆成准备 / 执行两步，命令行与 WebUI 共用）。

按用户要求没写新测试；只改了一处现有断言（自定义人格可见的平台级内置技能多了 gqy-cli、webui-theme）。

