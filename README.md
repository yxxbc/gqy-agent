<div align="center">

<img src="pics/gqy-logo.png" alt="GQY Logo" width="160" />

# GQY · Selene (顾清影)

**The Anime AI Companion Living in Your Terminal**  
**住在终端里的二次元 AI 伴侣**

Chatting · Remembering · Assisting · Coding with You  
陪你聊天 · 记得你 · 帮你办事 · 陪你写代码

<p align="center">
  <b>English</b> | <a href="README_zh.md">简体中文</a>
</p>

<p align="center">
  <a href="https://github.com/yxxbc/gqy-agent/releases/latest"><img src="https://img.shields.io/badge/version-0.7.0-blue.svg?style=flat" alt="Version"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-PolyForm%20NC%201.0.0-orange.svg?style=flat" alt="License: PolyForm Noncommercial 1.0.0"></a>
  <img src="https://img.shields.io/badge/platform-Linux%20%7C%20macOS-informational.svg?style=flat" alt="Platform">
  <a href="https://www.bestpractices.dev/projects/14831"><img src="https://www.bestpractices.dev/projects/14831/badge" alt="OpenSSF Best Practices"></a>
  <a href="https://scorecard.dev/viewer/?uri=github.com/yxxbc/gqy-agent"><img src="https://api.scorecard.dev/projects/github.com/yxxbc/gqy-agent/badge" alt="OpenSSF Scorecard"></a>
  <a href="https://slsa.dev/spec/v1.0/levels#build-l2"><img src="https://slsa.dev/images/gh-badge-level2.svg" alt="SLSA Build L2"></a>
  <a href="docs/longmemeval-results.md"><img src="https://img.shields.io/badge/LongMemEval-76.7%25%20(30--question%20sample)-lightgrey.svg?style=flat" alt="LongMemEval Benchmark"></a>
  <a href="docs/official-benchmark-guide.md"><img src="https://img.shields.io/badge/GAIA%20Harness-smoke%20test-lightgrey.svg?style=flat" alt="GAIA-format Harness (smoke test)"></a>
  <a href="SECURITY.md"><img src="https://img.shields.io/badge/Security-Policy%20Enabled-green.svg?style=flat" alt="Security Policy"></a>
  <a href="https://linux.do/t/topic/2947308"><img src="https://img.shields.io/badge/LINUX%20DO-Community-1c1c1e.svg?style=flat&logo=data:image/svg%2bxml;base64,PD94bWwgdmVyc2lvbj0iMS4wIiBlbmNvZGluZz0iVVRGLTgiPz48c3ZnIHZlcnNpb249IjEuMiIgYmFzZVByb2ZpbGU9InRpbnktcHMiIHdpZHRoPSIxMjgiIGhlaWdodD0iMTI4IiB2aWV3Qm94PSIwIDAgMTIwIDEyMCIgeG1sbnM9Imh0dHA6Ly93d3cudzMub3JnLzIwMDAvc3ZnIj48dGl0bGU+TElOVVggRE88L3RpdGxlPjxjbGlwUGF0aCBpZD0iYSI+PGNpcmNsZSBjeD0iNjAiIGN5PSI2MCIgcj0iNDciLz48L2NsaXBQYXRoPjxjaXJjbGUgZmlsbD0iI2YwZjBmMCIgY3g9IjYwIiBjeT0iNjAiIHI9IjUwIi8+PHJlY3QgZmlsbD0iIzFjMWMxZSIgY2xpcC1wYXRoPSJ1cmwoI2EpIiB4PSIxMCIgeT0iMTAiIHdpZHRoPSIxMDAiIGhlaWdodD0iMzAiLz48cmVjdCBmaWxsPSIjZjBmMGYwIiBjbGlwLXBhdGg9InVybCgjYSkiIHg9IjEwIiB5PSI0MCIgd2lkdGg9IjEwMCIgaGVpZ2h0PSI0MCIvPjxyZWN0IGZpbGw9IiNmZmIwMDMiIGNsaXAtcGF0aD0idXJsKCNhKSIgeD0iMTAiIHk9IjgwIiB3aWR0aD0iMTAwIiBoZWlnaHQ9IjMwIi8+PC9zdmc+" alt="LINUX DO"></a>
</p>

<img src="pics/gqy-tui.png" alt="GQY Terminal UI Screenshot" width="850" />

</div>

## 👋 Meet Selene (认识一下)

<img src="pics/gqy-mascot-is-not-the-tui-version..png" alt="GQY Mascot: Pixel Black Cat" width="170" align="right" />

Selene lives right in your computer terminal. She chats with you, remembers what you've shared, checks the weather, sets alarms, and tracks your daily accounts. When you need to write code, she focuses quietly and partners with you to get real engineering done.

> Her Chinese name **顾清影 (Gù Qīngyǐng)** and English name **Selene** are inspired by the classical verse *"起舞弄清影，何似在人间"* (dancing with clear shadows under the moon).

<br clear="right" />

---

## ✨ Features (她能做什么)

| Feature | Description | Reference / Docs |
| :-- | :-- | :-- |
| 💬 **Natural Chat (日常聊天)** | Expressive personality, witty, caring, and an empathetic listener | [Wiki: Quick Start](docs/wiki/01-快速开始.md) |
| 🧠 **Persistent Memory (真实长程记忆)** | Retains past dialogues and facts; remembers your corrections and reviews each chat afterwards | [Wiki: Memory](docs/wiki/08-记忆系统.md) |
| 🛠 **Autonomous Tools (日常助理)** | Weather, FX rates, maps, image generation, cron alarms, accounting | [Wiki: Overview](docs/wiki/02-功能总览.md) |
| 🧩 **Extensions (扩展)** | Skills, script tools, MCP servers and pm packages; toggle, inspect and update them in the Web UI | [Wiki: Tools & Plugins](docs/wiki/06-内置工具与插件.md) |
| 💻 **Dev Mode Pair-Programming (协同编程)** | Run `gqy dev` to inspect codebases, execute terminal tools, and edit code | [Wiki: CLI Reference](docs/wiki/04-命令参考.md) |
| 📚 **Private Knowledge Base (本地资料库)** | Ingest documents and query anytime; all data stays on your machine | [Wiki: Overview](docs/wiki/02-功能总览.md) |
| 🎙 **Local Voice Interaction (语音交互)** | Wake up with "清影" (Selene) for conversational speech; audio never leaves your machine | [Voice Manual (中文)](docs/voice.md) |
| 📱 **Everywhere You Are (跨平台接入)** | Terminal (TUI), Web browser, Mobile, QQ, and iMessage | [QQ & Platforms (中文)](docs/wiki/13-QQ与通讯平台.md) |

<div align="center">
<img src="pics/readme-show/tui-calltool.png" alt="Selene web search in terminal" width="760" />
<br/><sub>Real-time tool invocations directly in your terminal</sub>
<br/><br/>
<img src="pics/readme-show/tui-show-image.png" alt="Selene images in terminal" width="760" />
<br/><sub>Inline images and expressive memes rendered natively in terminal</sub>
<br/><br/>
<img src="pics/readme-show/tui-zsh-say.png" alt="Chat directly from shell command line" width="760" />
<br/><sub>Talk to Selene straight from your shell prompt without opening REPL</sub>
</div>

### 🖥 Web Dashboard (网页端控制台)

Access Selene anywhere in your browser — mobile and tablet friendly.

<div align="center">
<img src="pics/readme-show/webui-speak.png" alt="Web UI Chat View" width="760" />
<br/><sub>Clean, responsive Web chat interface</sub>
</div>

<table>
  <tr>
    <td align="center" width="33%"><img src="pics/readme-show/webui-create-agent.png" alt="Create custom persona" /><br/><sub>Custom Persona Builder</sub></td>
    <td align="center" width="33%"><img src="pics/readme-show/webui-show-usage.png" alt="Usage statistics and heatmap" /><br/><sub>Token & Cost Ledger</sub></td>
    <td align="center" width="33%"><img src="pics/readme-show/webui-setting-YouCanShowMore.png" alt="Settings & Console" /><br/><sub>Memory, Knowledge & Gallery</sub></td>
  </tr>
</table>

---

## 🏆 Benchmarks & Certifications (基准评测与权威认证)

GQY adheres to strict engineering rigor and open-source verification:

- 🛡️ **Linux Foundation OpenSSF Best Practices Certification**: Officially certified under [OpenSSF Best Practices Badge (Project 14831)](https://www.bestpractices.dev/projects/14831). Meets all requirements for security disclosure, memory safety, 2,300+ automated tests, and release management. See [OpenSSF Certification Guide (中文)](docs/openssf-certification-guide.md).
- 🔐 **Supply-chain security (供应链安全)**: continuously scored by [OpenSSF Scorecard](https://scorecard.dev/viewer/?uri=github.com/yxxbc/gqy-agent); every release tarball ships with Sigstore-signed SLSA build provenance (`gh attestation verify gqy-<platform>.tar.gz -R yxxbc/gqy-agent`); CodeQL SAST, cargo-deny (RustSec advisories / licenses / sources) and cargo-fuzz run in CI. See [section 4 of the OpenSSF guide (中文)](docs/openssf-certification-guide.md#4-自动化供应链与安全认证持续运行).
- 🧠 **LongMemEval (ICLR 2025 Long-Term Memory Benchmark)**: an internal reference run, not a leaderboard score. 23 of 30 sampled questions from `longmemeval_s_cleaned` were judged correct (**76.7%**), with gemini-flash as the judge and no baseline run under the same setup, so treat it as a sanity check only. See the evaluation and error analysis in [LongMemEval Results Report (中文)](docs/longmemeval-results.md).
- 🤖 **GAIA-format Evaluation Harness (General AI Assistants)**: Built-in sandboxed harness with multimodal extraction, answer normalization, and Hugging Face Leaderboard packaging. So far it has only been smoke-tested on 5 self-written GAIA-style samples; no score on the official GAIA validation set yet. See [Official Benchmark Guide (中文)](docs/official-benchmark-guide.md).

---

## 📦 Installation (快速安装)

Supported on **Linux** and **macOS**. [Nix](https://nixos.org/download/) is the main install path (Linux and Apple Silicon Macs); it downloads a prebuilt package:

```bash
nix profile install github:yxxbc/gqy-agent/gqy
```

No Nix, or on an Intel Mac? Use the one-line installer (installs to `~/.local`, no root; it refuses to install over an existing Nix install):

```bash
curl -fsSL https://raw.githubusercontent.com/yxxbc/gqy-agent/gqy/install.sh | sh
```

<img src="pics/gqy-install-show.png" alt="Installer Interface" width="640" />

<details>
<summary><code>gqy</code> command not found after installation? (找不到命令？)</summary>

Add this to your `~/.zshrc` or `~/.bashrc`, then restart your terminal:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

</details>

<details>
<summary>Alternative Installation Methods (其他安装方式)</summary>

- Try without installing (requires Nix): `nix run github:yxxbc/gqy-agent/gqy`
- Preview installer UI: append `-s -- --preview` to the curl command
- Manual binary download: grab tarballs from [Releases](https://github.com/yxxbc/gqy-agent/releases/latest) and place `bin`, `lib`, and `share` into `~/.local`

</details>

---

## 🌸 Quick Start (开始聊天)

Launch the interactive REPL (each launch opens a fresh session):

```bash
gqy
```

First-time launch guides you through a friendly setup: pick a persona (Selene or your own), tell her about yourself, optionally add shell integration, and link an AI model (Claude Code, Codex, Antigravity, DeepSeek, or any OpenAI-compatible endpoint).

### Everyday Cheat Sheet (常用命令)

| Action | Command / Shortcut | Description |
| :-- | :-- | :-- |
| Resume previous session | `gqy -c` | Pick up where you left off |
| Open a session by name | `gqy --session NAME` | Name, number or id |
| Dev Mode (coding assistant) | `gqy dev` | Focused pair-programming |
| Command palette | `/` | Slash commands, pick with the arrow keys |
| Settings & Config | `/config` | TUI configuration panel (`/config display` etc. jumps to one group) |
| Interrupt response | Press `Esc` twice | Instantly stop generation |

> [!TIP]
> We recommend the [Kitty](https://sw.kovidgoyal.net/kitty/) terminal for rich inline graphics rendering.

---

## 🌐 Other Channels (更多访问方式)

- **Web Dashboard**: Run `gqy web` and open the URL in your browser. First login: `gqy` / `GQY520`, then create your own admin account. Settings live under 控制台 (Console) → 设置 (Settings); color themes are under 界面 (Interface) → 配色方案, and Selene can write new ones for you.
- **Shell Direct Prompt**: Run `gqy zsh-init` (supports bash and fish too) to query Selene directly from your command line.
- **Voice Mode**: Enable in settings and wake with "清影" (Selene). See [Voice Setup (中文)](docs/voice.md).
- **QQ & iMessage**: Chat with Selene on mobile or add her to QQ groups. QQ is set up in the Web UI's 平台 (Platforms) page; iMessage (macOS only) goes through a small connector script in `scripts/imessage/` that talks to the daemon (`platforms.connectors.imessage`). See [QQ & Platforms Guide (中文)](docs/wiki/13-QQ与通讯平台.md) and [scripts/imessage/README.md](scripts/imessage/README.md).

---

## 🔄 Maintenance (升级、备份与卸载)

<details>
<summary>Click to expand maintenance guide (点击展开)</summary>

**Upgrade (升级)**:
- Nix installed: run `nix profile upgrade gqy-agent` (`nix profile rollback` undoes it).
- Script installed: re-run the `curl` installer.
- Release notes: see [CHANGELOG.md](CHANGELOG.md).

**Backup & Migration (备份与换电脑)**:

```bash
gqy export                      # Archives config, sessions, memory, and knowledge base
gqy import gqy-export-*.tar.gz  # Import on a new machine (run gqy daemon stop first)
```

> [!WARNING]
> Backup archives contain your configured API keys. Use `gqy export --no-secrets` when sharing.

**Uninstall (卸载)**:
Stop daemon with `gqy daemon stop`. Remove `~/.local/bin/gqy` (or `nix profile remove gqy-agent`). User data resides in `~/.gqy`.

</details>

---

## 📖 Documentation Center (文档中心)

Detailed documentation is hosted in `docs/` (primarily in Chinese):

- 🚀 [Quick Start / 快速开始](docs/wiki/01-快速开始.md)
- 🧭 [Feature Overview / 功能总览](docs/wiki/02-功能总览.md)
- ⌨️ [CLI Reference / 命令参考](docs/wiki/04-命令参考.md)
- 🔒 [Security & Privacy Policy / 安全与隐私说明](docs/wiki/16-安全与隐私.md)
- 🛡️ [Security Vulnerability Policy / 漏洞通报指南](SECURITY.md)
- 🏅 [OpenSSF Best Practices Guide / OpenSSF 认证申报指南](docs/openssf-certification-guide.md)
- 📊 [LongMemEval Benchmark Results / 长程记忆跑分报告](docs/longmemeval-results.md)
- 🤖 [Official Benchmark Guide (GAIA/AML) / 官方基准打榜指南](docs/official-benchmark-guide.md)
- ❓ [FAQ / 常见问题解答](docs/wiki/17-常见问题.md)
- 🛠️ [Developer Guide / 参与开发](docs/wiki/14-参与开发.md) & [Contributing / 贡献指南](CONTRIBUTING.md)

---

## 💐 Acknowledgements (致谢)

GQY is developed and refactored from [shorin/miyu-agent 0.6.0](https://github.com/SHORiN-KiWATA/miyu-agent).

<details>
<summary>Referenced Projects (参考过的开源项目)</summary>

Architecture & Concepts:
[Opencode](https://github.com/anomalyco/opencode) ·
[Claude Code](https://github.com/anthropics/claude-code) ·
[Pi](https://github.com/earendil-works/pi) ·
[Deepseek-Reasonix](https://github.com/esengine/deepseek-reasonix) ·
[Deepseek-Harness](https://github.com/deepseek-ai/deepseek-harness) ·
[AstrBot](https://github.com/AstrBotDevs/AstrBot) ·
[NapCatQQ](https://github.com/NapNeko/NapCatQQ)

Plugins & Ecosystem:
[astrbot_plugin_maskoff](https://github.com/Yue-bin/astrbot_plugin_maskoff) ·
[astrbot_plugin_GroupMemberQuery](https://github.com/nuomicici/astrbot_plugin_GroupMemberQuery) ·
[Astrbot_plugin_Heartflow](https://github.com/advent259141/Astrbot_plugin_Heartflow) ·
[astrbot_plugin_image_generation](https://github.com/Railgun19457/astrbot_plugin_image_generation) ·
[astrbot_plugin_weather_wttr_in](https://github.com/xiewoc/astrbot_plugin_weather_wttr_in) ·
[astrbot_plugin_recall_cancel](https://github.com/muyouzhi6/astrbot_plugin_recall_cancel)

</details>

---

## 📄 License (开源协议)

- Code: [PolyForm Noncommercial 1.0.0](LICENSE) (free for personal, educational, and research use; non-commercial).
- Brand Assets: Selene / 顾清影 persona, illustrations, wallpapers, and logos are governed separately under [LICENSE-ASSETS](LICENSE-ASSETS).
- Legacy Core: Derived from [miyu-agent](https://github.com/SHORiN-KiWATA/miyu-agent), original MIT license retained in [LICENSE-MIT](LICENSE-MIT).

---

## Friendly Links (友情链接)
[linux.do](https://linux.do)