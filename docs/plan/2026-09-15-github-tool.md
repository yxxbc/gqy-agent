# 2026-09-15 · github 工具与 顾清影 的 bot 身份

对应 todolist「给 顾清影 安排一个专属 github 工具」。

## 用户拍板

| 问题 | 决定 |
|---|---|
| 形态 | 结构化 `github` 工具 + 身份隔离（署名由代码保证，不靠提示词） |
| bot 身份何时生效 | 用户明确要求时（`as_bot=true`），默认用用户自己的 gh / git |
| 署名方向 | 固定用户 author，顾清影 `Co-Authored-By` |
| bot 凭据 | `~/.gqy/github/` 下独立 gh 目录，`gqy github login` 管理 |

## 施工

- `src/tools/github/`：`identity.rs`（身份与环境隔离）、`attribution.rs`（trailer）、`actions.rs`（status / commit / pr_create / issue_create / comment / gh）、`tests.rs`。
- 注册在 core（dev 人格也有），trust 缺省 Owner，不进受限平台面。形状夹具 normal / dev 各多一条 `github`，这是一次计划内的缓存冷启动。
- `workspace::TurnModel` task-local：回合入口写入主模型与上下文窗口，trailer 名字形如 `顾清影【claude-opus-5】 (200K)`。
- 配置 `tools.github.{enabled, coauthor_name, coauthor_email}`。
- CLI `gqy github login | status | logout`。

## 与 todolist 原写法的偏差

原写法 `<noreply@https://github.com/yxxbc/gqy-agent>` 不是合法邮箱。GitHub 只按邮箱关联账号，这样写不会挂头像、不计贡献。改为：配置邮箱 → bot 的 `<id>+<login>@users.noreply.github.com` → 兜底 `gqy-agent@noreply.invalid`（保留域，不会挂错人）。

## 隔离实测（09-15，macOS，gh 2.100.0，Homebrew git 2.55.0）

1. Homebrew 的 `/opt/homebrew/etc/gitconfig` 带 `credential.helper=osxkeychain`，所以 bot 模式设 `GIT_CONFIG_NOSYSTEM=1`，bot gitconfig 用空 `helper =` 清掉继承链。
2. **gh 的 Keychain 回退**：`GH_CONFIG_DIR` 指向空目录时，`gh auth status` 显示未登录，但 `gh auth git-credential get` 仍然交出宿主 Keychain 里的 token（哈希与宿主 `gh auth token` 一致）。bot 目录有 hosts.yml token 时用的是 bot token，显式 `GH_TOKEN` 优先级最高。
   → 修法：bot 模式从 bot 的 hosts.yml 读 token，读不到就拒绝，读到就用 `GH_TOKEN` 显式注入。`gqy github login` 取账号时同样处理。回归用例 `bot_without_a_stored_token_is_refused`。
3. bot 模式 `GIT_SSH_COMMAND=false`：SSH 远端会拿宿主私钥推送，bot 一律走 HTTPS。

## 施工中踩到的坑

- 回合入口多套一层 `with_turn_model` 后，`agent::tests::context::compaction_resets_the_byte_prefix_at_most_once_each` 在 2MB 测试线程上栈溢出（单跑稳定复现）。原因是回合 future 按值嵌套。内层改为 `Box::pin` 后通过。
- `test_scripts/refactor-check.sh` 第 41 行 `$now——` 紧贴全角字符，bash 报「未绑定的变量」。只有用例数下降时才会走到这里，是脚本的存量问题，本次没改。

## 验收

- 09-15 release 构建装入 `~/.cargo/bin/gqy`（旧版备份 `~/.gqy/bin-backup/gqy-0.6.0-20260914-1710`），daemon 重启，用户验收通过。
- 门禁：lib 2353 通过、0 失败，用例数 2337 → 2356。格式、编译、模型面英文、文件规模、依赖方向全绿（后三道因脚本存量问题手动补跑）。
- 已写入 `next-release-note.md`。
