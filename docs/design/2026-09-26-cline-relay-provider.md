# Cline CLI 中转供应商（2026-09-26，施工中）

来由：用户要求「添加内置供应商，就是本地的 cline 调用，和 claude 那几条一样的
原理」；同一个提交里把 `docs/plan/2026-09-24-webui-provider-icons.md` 的品牌图标
一并做了。续传策略用户拍板：**v1 就做**（发现不对自动退化全量重放）。

## 一、事实与探针（09-26 实机，cline 3.0.65）

- `cline --json "<prompt>"` 输出 NDJSON，每行一个 JSON：
  - `{"type":"agent_event","event":{…}}`——事件就是 `@cline/shared` 的
    `AgentEvent`：`content_start|content_update|content_end`（contentType =
    text/reasoning/tool）、`usage`、`done`、`error`、`notice`、`iteration_*`；
  - `{"type":"hook_event",…}`、`{"type":"team_event",…}`——与中转无关。
- `--json` 必须有 prompt 参数：只喂 stdin 会直接报
  `JSON output mode requires a prompt argument or piped stdin`（与 README 的
  「或 piped stdin」不符，实测以二进制为准）。失败时退出码 1，stderr 上一行
  `{"type":"error","message":…}`。
- 登录态与数据在 `~/.cline`（`CLINE_DATA_DIR` 可改数据目录）；MCP 设置文件路径
  可用 `CLINE_MCP_SETTINGS_PATH` 指向任意文件，条目形状
  `{type:"stdio", command, args, env}`。
- 会话文件 `~/.cline/data/sessions/<id>/<id>.json` 带
  `session_id`/`prompt`/`cwd`/`pid`/`status`；事件流里**没有**会话 id。
- 模型目录：`cline config --json` 只认 workflows/rules/skills/agents/plugins/
  hooks/mcp/tools 这些 target，且要真 TTY；真正的目录在 cline 本体自带的
  `@cline/llms`（`getModelsForProvider(providerId)`，TUI 的模型选择器同源）。
  从 `<prefix>/bin/cline`（符号链接）的解真实路径向上找
  `node_modules/@cline/llms/dist/index.js`，用 node 跑一段小脚本即可取到全部
  模型 id（含 contextWindow/capabilities）；`cline` 317 个、`cline-pass` 18 个
  （09-26 实测）。

## 二、设计

- **第四条 CLI 中转线**，骨架全复用 `cli_relay`（工具作用域、哈希链、子进程泵、
  沙盒关押）；cline 特有的三样放在 `llm/openai_compatible/cline/`：
  参数拼装（`mod.rs`）、NDJSON 解析（`stream.rs`）、会话发现与核对（`session.rs`）。
- 载荷：位置参数，96 KiB 字节预算（Linux 单参数 128 KiB 上限）；媒体块降级成
  占位文本。
- 工具桥：`CLINE_MCP_SETTINGS_PATH` 指向本轮临时配置文件（只含 `gqy` 一条，
  与 claude 线 `--strict-mcp-config` 同义），进程收口即删；`GQY_MCP_EXCLUDE`
  去重名单与 claude 线一致。
- 续传：首轮全量重放 → 按 `prompt`（逐字）＋`cwd` 在会话目录里发现本轮会话 id
  → 记入 `cli_relay::session` 哈希链；此后 `--id` 只发增量。每轮结束后核对目标
  会话确实被本轮写过（目录/元数据/消息流 mtime 或 `prompt` 相符）；核对不过就
  忘链并把该供应商的续传拉闸（进程内不再记映射，全量重放到底）。
- 模型目录：`config_tui::cli_catalog` 增一条 cline 分支，起 `node` 读 cline 自带
  的 `@cline/llms`（定位法见 §一），与 agy/codex 的 live 目录同一档：拉不到就
  报错，不悄悄退回快照。
- 沙盒：`relay_config_grants` 增加 `~/.cline`。

## 三、已知限制（有意为之）

- 模型目录依赖 cline 自带的 `@cline/llms` 与 `node`：包不在（安装不完整）或
  node 不在 PATH 时，目录报错、退到手工添加；预置表为空是刻意的（模型 id 由
  用户自己的 cline 供应商决定，不把某家的模型当通用预置）。
- 媒体消息只发占位文本；模型要看图靠它自己的文件工具。
- 辅助请求（compact/judge/title）也在 cline 侧落会话（CLI 没有 ephemeral 旗标）。
- 会话文件布局是 CLI 内部实现：对不上只是不续传，不上抛错误。