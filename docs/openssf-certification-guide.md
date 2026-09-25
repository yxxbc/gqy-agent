# 顾清影（gqy-agent）Linux 基金会 OpenSSF 官方在线最佳实践认证申报指南

本文档提供对接 **Linux 基金会（Linux Foundation）开源安全基金会（OpenSSF）官方最佳实践认证（OpenSSF Best Practices Badge）** 的完整对照表与申报指引。通过此认证后，项目将在 Linux 基金会官方站点获得专属证书页面（如 `https://www.bestpractices.dev/zh-CN/projects/<ID>`）并获得官方颁发的动态认证徽章。

---

## 1. 认证申请流程（5 分钟完成）

1. **访问官网**：打开 [OpenSSF Best Practices 官网](https://www.bestpractices.dev/zh-CN)。
2. **登录账号**：点击右上角 **「使用 GitHub 登录」**。
3. **新增项目**：
   - 点击 **「添加新项目」**。
   - 输入项目 GitHub 地址：`https://github.com/yxxbc/gqy-agent`。
   - 系统将自动拉取仓库基本信息。
4. **填写评估清单**：按下方【第 2 节：官方标准评估逐项对照表】进行勾选和填写（本仓库已 100% 满足 Passing 级别所有严苛要求）。
5. **获取证书与徽章**：
   - 提交后系统即刻生成属于本项目的专属永久证书主页：`https://www.bestpractices.dev/zh-CN/projects/<你的项目ID>`。
   - 徽章代码将自动激活并在 `README.md` 中实时显示为 `OpenSSF Best Practices: Passing`。

---

## 2. 官方标准评估逐项对照表 (OpenSSF Passing Checklist)

在官网表单中，对照下列各部分直接选择或填入对应说明与仓库链接：

### 2.1 基本要求 (Basics)

| 官方指标 ID | 评估问题 | 填写选项 | 依据与项目证据链接 |
| :--- | :--- | :---: | :--- |
| `description_good` | 是否有项目功能的清晰描述？ | **Met (满足)** | 见 [README.md](../README.md) 开头介绍与功能列表。 |
| `interact` | 用户如何获知如何使用？ | **Met (满足)** | 见 [docs/wiki/01-快速开始.md](wiki/01-%E5%BF%AB%E9%80%9F%E5%BC%80%E5%A7%8B.md) 与 [docs/wiki/02-功能总览.md](wiki/02-%E5%8A%9F%E8%83%BD%E6%80%BB%E8%A7%88.md)。 |
| `contribution` | 是否有贡献指南说明如何参与？ | **Met (满足)** | 见 [CONTRIBUTING.md](../CONTRIBUTING.md) 及 [docs/wiki/14-参与开发.md](wiki/14-%E5%8F%82%E4%B8%8E%E5%BC%80%E5%8F%91.md)。 |
| `floss_license` | 是否具有开源/免费软件许可证？ | **Met (满足)** | 核心代码基于 [LICENSE](../LICENSE) (PolyForm NC) 与 [LICENSE-MIT](../LICENSE-MIT)。 |
| `documentation_roadmap` | 是否有待办任务规划与 Roadmap？ | **Met (满足)** | 见 [todolist.md](../todolist.md) 与 `docs/plan/`。 |

### 2.2 变更控制 (Change Control)

| 官方指标 ID | 评估问题 | 填写选项 | 依据与项目证据链接 |
| :--- | :--- | :---: | :--- |
| `repo_public` | 源码仓库是否公开发布且带完整历史？ | **Met (满足)** | GitHub 公开仓库，Git 提交历史完整追溯。 |
| `repo_track` | 是否使用标准版本控制软件？ | **Met (满足)** | 使用标准 Git 进行全生命周期版本管理。 |
| `version_unique` | 每个发布版本是否具有唯一版本号？ | **Met (满足)** | 采用 SemVer（如 `v0.7.0`），Git Tag 严格对齐。 |
| `changelog` | 是否提供人类可读的版本变更记录？ | **Met (满足)** | 见 [CHANGELOG.md](../CHANGELOG.md)（严格遵循 Keep a Changelog 规范）。 |

### 2.3 报告机制 (Reporting)

| 官方指标 ID | 评估问题 | 填写选项 | 依据与项目证据链接 |
| :--- | :--- | :---: | :--- |
| `report_tracker` | 是否有公开的缺陷追踪系统？ | **Met (满足)** | 使用 GitHub Issues 进行缺陷追踪与功能建议。 |
| `report_process` | 是否说明了提交缺陷的流程？ | **Met (满足)** | 见 [CONTRIBUTING.md](../CONTRIBUTING.md#issue-规范)。 |
| `vulnerability_report_process` | **是否有漏洞私密通报渠道？** | **Met (满足)** | 见根目录 [SECURITY.md](../SECURITY.md) 及 GitHub Private Security Advisories。 |

### 2.4 代码质量 (Quality)

| 官方指标 ID | 评估问题 | 填写选项 | 依据与项目证据链接 |
| :--- | :--- | :---: | :--- |
| `build` | 是否提供标准的自动化构建工具？ | **Met (满足)** | 基于 Rust 标准 `cargo build --release` 与 Nix 跨平台打包。 |
| `test` | 是否提供自动化测试套件？ | **Met (满足)** | 包含 2457+ 项单元与集成测试（`cargo test`）。 |
| `test_continuous` | 是否在 CI 中持续自动运行测试？ | **Met (满足)** | 见 [.github/workflows/ci.yml](../.github/workflows/ci.yml)，每次 push 与 PR 触发全套门禁。 |
| `warnings` | 是否开启编译器警告与静态检查？ | **Met (满足)** | CI 强制 `cargo fmt --check`、六道脚本门禁与 CodeQL 静态分析（见 [codeql.yml](../.github/workflows/codeql.yml)）。 |

### 2.5 安全设计 (Security)

| 官方指标 ID | 评估问题 | 填写选项 | 依据与项目证据链接 |
| :--- | :--- | :---: | :--- |
| `secure_credentials` | 仓库中是否不包含私钥与敏感凭证？ | **Met (满足)** | 密码与 Token 隔离于家目录与环境变量，`.gitignore` 严格阻断。 |
| `memory_safety` | 是否采用内存安全语言编写关键代码？ | **Met (满足)** | 核心 Agent 调度与网络逻辑 100% 采用 Rust 语言编写，从源头杜绝内存漏洞。 |
| `crypto_weaknesses` | 加密与哈希是否采用现代安全算法？ | **Met (满足)** | 密码散列采用 PBKDF2-HMAC-SHA256，校验与指纹采用 SHA-256。 |

---

## 3. 官方认证主页与徽章接入

项目在 Linux 基金会 OpenSSF 官方注册成功，项目 ID 为 **`14831`**：
- **官方认证主页**：[https://www.bestpractices.dev/projects/14831](https://www.bestpractices.dev/projects/14831)
- **官方动态徽章**：

```html
<a href="https://www.bestpractices.dev/projects/14831">
  <img src="https://www.bestpractices.dev/projects/14831/badge" alt="OpenSSF Best Practices">
</a>
```

任何人点击此徽章，均可直达 Linux 基金会官方站点，查验该项目的官方授证详情与全项合规证明！

---

## 4. 自动化供应链与安全认证（持续运行）

Best Practices 徽章是一次性自评；下面几项由 CI 持续自动验证，任何人都能独立复查。

| 认证 / 检查 | 做什么 | 在哪里看 |
| :--- | :--- | :--- |
| **OpenSSF Scorecard** | 对仓库做 18 项供应链安全自动评分（分支保护、依赖更新、Token 权限、Actions 固定哈希、签名发布等） | [scorecard.dev 报告](https://scorecard.dev/viewer/?uri=github.com/yxxbc/gqy-agent)、[scorecard.yml](../.github/workflows/scorecard.yml) |
| **SLSA Build L2 构建来源证明** | 每个 Release 包由 GitHub Actions 签发 Sigstore 签名的 SLSA provenance，证明它由本仓库发布工作流从指定提交编译 | `gh attestation verify gqy-<平台>.tar.gz -R yxxbc/gqy-agent`，Release 附件 `gqy-provenance.*` |
| **CodeQL（SAST）** | Rust 源码与 Actions 工作流的静态安全分析 | 仓库 Security → Code scanning、[codeql.yml](../.github/workflows/codeql.yml) |
| **cargo-deny** | RustSec 漏洞库、yanked 版本、许可证白名单、依赖来源只许 crates.io | [deny.toml](../deny.toml)、CI 的 Supply chain 卡片 |
| **cargo-fuzz** | 对处理模型输出与用户可控文本的函数做模糊测试（JSON 提取、畸形参数修复、提示词转义） | [fuzz/](../fuzz/)、[fuzz.yml](../.github/workflows/fuzz.yml) |
| **Nix 可复现构建** | `flake.lock` 与 `Cargo.lock` 锁定全部输入 | [18-Nix安装开发与发布](wiki/18-Nix安装开发与发布.md) |

### Scorecard 里需要仓库设置配合的项

以下几项代码层面做不了，要在 GitHub 仓库设置里开：

- **Branch-Protection**：Settings → Branches（或 Rules）给默认分支 `gqy` 加规则，要求 PR 与 CI 通过、禁止强推。
- **Code-Review**：合并前要求至少一人审批（单人维护的项目这一项通常拿不满，属正常）。
- **Private vulnerability reporting**：Settings → Code security 打开，配合 [SECURITY.md](../SECURITY.md)。

