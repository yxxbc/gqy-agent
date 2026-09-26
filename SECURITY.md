# 安全策略与漏洞通报指南 (Security Policy)

顾清影（GQY Agent）视用户的数据安全与隐私保护为核心生命线。本文档说明我们支持的安全版本范围、安全漏洞的负责任通报流程以及项目的安全设计架构。

---

## 1. 支持的版本 (Supported Versions)

我们仅针对最新正式发布版本及当前开发主分支提供活跃的安全补丁支持：

| 版本 | 活跃支持状态 |
| :--- | :---: |
| 当前最新版本 (`>= 0.7.0`) | :white_check_mark: 推荐使用并持续维护 |
| `0.6.x` 及更早版本 | :x: 已终止安全支持，请尽快升级 |

---

## 2. 漏洞报告渠道 (Reporting a Vulnerability)

如果您在 顾清影 中发现了潜在的安全漏洞（包括但不限于：远程代码执行、权限逃逸、凭证泄漏、沙箱穿透、SSRF 或提权隐患），**请勿直接在 GitHub 公开 Issues 中提报**。

请通过以下方式提交负责任的漏洞报告：

1. **GitHub Private Security Advisory（推荐）**：
   - 访问仓库的 [Security Advisories 页面](https://github.com/yxxbc/gqy-agent/security/advisories)。
   - 点击 **"Report a vulnerability"** 提交私密报告。
2. **私密邮件沟通**：
   - 发送邮件至安全维护者邮箱：`xynrin@outlook.com`
   - 邮件主题请注明：`[SECURITY VULNERABILITY] GQY Agent - <漏洞简述>`
   - 请在报告中尽可能提供：受影响组件与版本、详细复现步骤 (PoC)、潜在危害评估。

### 响应时效承诺
- **确认收到**：维护团队将在 **48 小时** 内回复确认收到您的报告。
- **验证与评估**：在 **5 个工作日** 内完成漏洞有效性验证并给出初步评级。
- **修复与披露**：高危漏洞将在修复分支上优先验证，并在发布安全补丁版本（如 Hotfix 发布）后进行负责任的协同安全通报（Coordinated Vulnerability Disclosure）。

---

## 3. 安全设计护栏与架构原则 (Security Architecture)

顾清影在架构底层遵循以下防御设计：
1. **Rust 内存安全**：核心 Agent 循环、网络调度与协议处理均采用 Rust 编写，根除缓冲区溢出与野指针等内存破坏缺陷。
2. **凭据脱敏与隔离**：
   - API Key 与密码仅存储于权限受限目录（`~/.gqy/config/config.jsonc`，权限 `0700`）。
   - `gqy export` 的归档默认**包含**密钥（明文 tar.gz，权限 `0600`，程序会警告），分享前用 `--no-secrets` 清空。
   - 请求日志脱敏：默认仅记录 Token 统计，严格禁止向持久化日志中写入用户明文提示词正文。
3. **命令执行与沙箱护栏**：
   - 外部命令均具备超时截断、独立进程组隔离与黑名单阻断（`tools.command_deny`）。
   - 文件破坏防护：禁止对根目录 `/`、用户家目录等关键系统路径执行递归删除，危险文件操作强制走系统回收站。
4. **通讯平台权限隔离（如 QQ / 外部服务）**：
   - 身份验证基于平台强凭据（Principal / 账号 ID），严格防范冒名欺诈。
   - 平台端默认采用受限工具面，隔离本地终端高危命令能力，防止越权滥用。
5. **WebUI 与多用户**：
   - WebUI 永远要登录；密码 PBKDF2 存库，登录令牌只存 sha256。
   - 成员回合与绑定了 `/sandbox` 的会话套 Landlock 沙盒（Linux），读写都限制在工作区与必需的系统目录。
   - WebUI 主题只开放 CSS，不开放前端脚本；CSP 为 `script-src 'self'; style-src 'self'`。

---

## 4. 供应链与构建安全 (Supply Chain)

- **依赖审计**：CI 每次推送运行 cargo-deny（`deny.toml`：RustSec 漏洞库、许可证白名单、依赖来源仅限 crates.io）。
- **静态分析与评分**：CodeQL（`.github/workflows/codeql.yml`）与 OpenSSF Scorecard（`scorecard.yml`）。
- **模糊测试**：`fuzz/` 下的 cargo-fuzz 目标（JSON 提取、参数形状还原、`safe_prompt_field`），每周定时及相关代码变动时运行（`fuzz.yml`）。
- **构建来源证明**：Release 由 GitHub Actions 云端构建，附 SLSA provenance（`gqy-provenance.sigstore.json`、`gqy-provenance.intoto.jsonl`），可用 `gh attestation verify gqy-<平台>.tar.gz -R yxxbc/gqy-agent` 校验。
