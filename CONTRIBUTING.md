# 参与贡献

谢谢你愿意帮顾清影变得更好！这是一个业余维护的个人项目，下面这些约定是为了让 issue 和 PR 都能被快速处理。

## 提 issue 之前

- 先看 [常见问题](docs/wiki/17-常见问题.md)，再搜一下已有的 [issue](https://github.com/yxxbc/gqy-agent/issues)，避免重复。
- 用对应的模板提：**问题反馈** 或 **功能建议**。空白 issue 已关闭。
- 一个 issue 只说一件事。
- 反馈问题时请写清楚：
  - 版本（`gqy --version`）、系统、终端（kitty / iTerm2 / Ghostty…）、安装方式（Nix / 安装脚本 / 源码）
  - 怎么复现：一步步做了什么、期望看到什么、实际看到什么
  - 相关日志：`~/.gqy/cache/logs/` 下当天的 `gqy.日期.log` 和 `daemon.log`
- **贴日志和截图前先打码**：API key、token、QQ 号、聊天内容、个人路径。使用问题请不要附带 `gqy export` 的备份文件，里面有明文密钥。
- 安全漏洞请不要公开提 issue，通过 GitHub 的 [私密漏洞报告](https://github.com/yxxbc/gqy-agent/security/advisories/new) 告诉我。

## 提 PR 之前

- **大改动先开 issue 商量**：新功能、改交互、改人格与提示词、改数据库结构、加依赖。小的 bug 修复和文档修正可以直接提。
- 从 `gqy` 分支拉出你的分支，PR 也提到 `gqy` 分支。
- 一个 PR 只做一件事，别把无关的格式化、重命名混进来。

## 开发约定

完整的工程规范在 [AGENTS.md](AGENTS.md)（原本是写给编码代理的，人也适用），环境搭建见 [参与开发](docs/wiki/14-参与开发.md)。最常碰到的几条：

- **格式与检查**：提交前跑 `cargo fmt`，`cargo clippy` 不新增警告。
- **测试**：修 bug 先写一个能复现的测试，确认修之前它是红的；新功能要有测试。CI 会在 Linux 和 macOS 上跑全套测试，必须全绿才会合并。
- **文件别写太大**：单个文件目标 800 行以内，超过 1500 行要拆，2000 行是红线。
- **提示词与缓存**：模型能看到的文字（工具描述、注入内容）要保持逐字节稳定，不能拼时间、随机数、本机路径进去，否则会让提示词缓存失效。改这一块前先读 AGENTS.md 第 1 节。
- **工具**：新增或删除工具要同时改 `src/tools/descriptions/*.json`、`config/plugin_catalog.rs`、`tools/compose.rs`，见 [扩展指南](docs/wiki/15-扩展指南.md)。
- **数据库**：迁移只能在末尾追加，只增不删。
- **前后端一起改**：一个功能如果终端界面和网页都有，两边要同步。
- **不要改**：`nix/release.json`（只由脚本生成）、`scripts/imessage/` 的路径。

## 提交信息

用 [Conventional Commits](https://www.conventionalcommits.org/zh-hans/) 格式，英文书写：

```
feat(tui): pick slash commands with the arrow keys

说明为什么改、改了什么、有什么影响。
```

常用类型：`feat` 新功能、`fix` 修复、`perf` 性能、`refactor` 重构、`docs` 文档、`test` 测试、`chore` 杂项。

## 更新日志

用户能感知到的改动，在 PR 里同时更新 [CHANGELOG.md](CHANGELOG.md) 的 `[Unreleased]`，写法见文件开头：写给用户看，说清改了什么、对用户有什么影响。纯内部重构、文档、CI 不用写。

## 贡献的授权

本项目采用 [PolyForm Noncommercial 1.0.0](LICENSE) 协议。提交 PR 即表示你同意：

1. 你有权提交这些内容（是你自己写的，或者来源的协议允许这样用）。
2. 你的贡献按本项目的协议发布。
3. 你同时授予项目维护者（yxxbc）一项永久、全球、免费、不可撤销的许可，可以以任何协议（包括商业协议）使用、修改、再授权和发布你的贡献。这样项目以后调整协议时，不用逐个联系贡献者。

你保留自己贡献的版权。
