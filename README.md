<div align="center">

<img src="pics/gqy-logo.png" alt="GQY Logo" width="160" />

# GQY (顾清影)

**活在终端里的二次元 AI 伴侣 · 开箱即用 · 双模式设计 · 多端接入**

*A lightweight, terminal-first anime AI assistant built with Rust.*

<p align="center">
  <a href="https://github.com/yxxbc/gqy-agent"><img src="https://img.shields.io/badge/version-0.6.0-blue.svg?style=flat" alt="Version"></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/rust-1.89+-DEA584.svg?style=flat&logo=rust&logoColor=white" alt="Rust Version"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-green.svg?style=flat" alt="License"></a>
  <img src="https://img.shields.io/badge/platform-Linux%20%7C%20macOS-informational.svg?style=flat" alt="Platform">
</p>

<img src="pics/gqy-tui.png" alt="GQY TUI Screenshot" width="850" />

</div>

---

## 📑 目录

- [📖 项目简介](#-项目简介)
- [✨ 核心特性](#-核心特性)
- [🚀 快速开始](#-快速开始)
  - [环境准备](#环境准备)
  - [从源码构建](#从源码构建)
  - [初始化与启动](#初始化与启动)
- [🕹️ 交互模式](#️-交互模式)
  - [1. 终端 TUI / REPL](#1-终端-tui--repl)
  - [2. 局域网 WebUI](#2-局域网-webui)
  - [3. Shell 终端集成](#3-shell-终端集成)
  - [4. 本地离线语音 (Voice)](#4-本地离线语音-voice)
- [⚙️ 配置与个性化](#️-配置与个性化)
- [📦 数据迁移与备份](#-数据迁移与备份)
- [💖 致谢与鸣谢](#-致谢与鸣谢)
- [📄 开源协议](#-开源协议)

---

## 📖 项目简介

**GQY（顾清影）** 是一款以 Rust 编写的高性能、轻量级 AI 智能体应用。

> 💡 **角色背景**：「顾清影」最初源于作者高中时期接触 AI 时由 `Gemini-2.5-pro` 生成的虚构角色。现在，她化身为你系统中的常驻 AI 伴侣——既能在日常对话中提供贴心陪伴与生活辅助，也能在编码排障时切换为高效纯粹的开发者助手。

本项目基础架构与命令系统基于 [shorin/miyu-agent 0.6.0](https://github.com/SHORiN-KiWATA/miyu-agent) 进行深度重构与二改开发。

---

## ✨ 核心特性

- 🎭 **双模式架构设计**
  - **Normal（普通模式）**：全功能与工具链开放，包含角色扮演、情感互动、游戏娱乐、天气汇率查询及日常生活辅助。
  - **Dev（开发模式）**：彻底隔离非开发工具与冗余提示词，以极简设计最大化释放大语言模型自身的代码推理与工程排障能力。
- 🧠 **灵活的模型生态**
  - 广泛兼容主流 OpenAI / Anthropic 协议中转。
  - 支持调用本地模型环境（如 `Claude Code`、`agy`、`codex`）。
  - 内置多档位模型池路由（Lite / Cheap / Standard / Flagship）。
- 🎙️ **端侧离线语音支持**
  - 搭载 **SenseVoice** 本地离线语音识别（零数据上传，彻底保护隐私）。
  - 支持常驻低功耗麦克风唤醒词监听、桌面通知提醒与 MiniMax / 小米 MiMo 语音合成（TTS）。
- 💾 **长期记忆与知识沉淀**
  - 具备好感度/情绪机制、会话联想注入与回合后经历/日记归档。
  - 结合本地 ONNX Runtime 向量模型实现本地离线知识库检索（RAG）。
- 🛠️ **完善的工具与插件生态**
  - 内置 MCP 客户端、异步命令与后台任务管理、文件 Patch 工具、定时闹钟、Web 抓取与图像生成等。

---

## 🚀 快速开始

### 环境准备

- **Rust 工具链**：1.89 及以上版本（附带 `cargo`）
- **操作系统**：Linux / macOS
- *(推荐)* **终端模拟器**：[Kitty](https://sw.kovidgoyal.net/kitty/)（可获得最佳的终端图文渲染体验）

### 从源码构建

```bash
# 1. 克隆代码仓库
git clone https://github.com/yxxbc/gqy-agent.git
cd gqy-agent

# 2. 修改默认人格提示词
cd src/prompts/
vim gqy.md

# 3. 编译主程序 (只生成 gqy)
cargo build --release

# (可选) 编译带语音特性的版本 (额外生成 gqy-voice，链接 sherpa-onnx)
cargo build --release --features voice
```

> [!TIP]
> 编译完成后，建议将 `target/release/gqy`（以及可选的 `gqy-voice`）放置在相同的系统 `PATH` 路径下（如 `~/.local/bin` 或 `/usr/local/bin`）。GQY 守护进程启动时会自动在同级目录寻找 `gqy-voice`。

### 初始化与启动

```bash
# 初始化配置与状态数据文件
gqy init

# 启动后台守护进程 (首次运行也会自动执行初始化)
gqy daemon start

# 查看 CLI 完整帮助
gqy -h
```

---

## 🕹️ 交互模式

GQY 提供了多样化的交互方式，无缝融入日常工作流：

### 1. 终端 TUI / REPL

```bash
gqy        # 进入 Normal 普通模式 REPL
gqy dev    # 进入 Dev 极简开发模式 REPL
```

### 2. 局域网 WebUI

轻量响应式 Web 操作界面，便于手机、平板或其他局域网设备接入：

```bash
gqy web
```

> [!NOTE]
> 首次访问会提示登录内置初始账号（默认用户名与密码均为 `gqy`），创建属于你的管理员账号后，初始账号将自动删除。

### 3. Shell 终端集成

无需离开终端即可直接与 GQY 对话：

```bash
# 生成并配置 zsh 集成脚本
gqy zsh-init
```

- **zsh**：提供无缝嵌入式对话支持。
- **fish / bash**：支持单行快速问答。

### 4. 本地离线语音 (Voice)

在配置中开启「语音功能」后，Daemon 会自动拉起独立的 `gqy-voice` 常驻进程：

- **语音唤醒**：呼叫唤醒词（默认 *清影* / *顾清影* / *清影清影*） $\rightarrow$ 提示音 + 桌面通知 $\rightarrow$ 说出指令 $\rightarrow$ 执行并播报回复摘要。
- **本地听写**：REPL 中执行 `/stt`、命令行输入 `gqy stt` 或点击 WebUI 麦克风均可快速听写。
- **快捷收听**：支持通过 `gqy listen` 绑定全局快捷键一键唤起。
- 更多详细配置请参阅 [`docs/voice.md`](docs/voice.md)。

---

## ⚙️ 配置与个性化

运行以下命令调出可视化的交互式配置终端（TUI）：

```bash
gqy config
```

- **供应商与模型设置**：默认提供 opencode 公共 API，推荐配置个人 API 密钥以获得更稳定的服务体验。
- **自定义提示词与人设**：支持在「自定义提示词」中创建专属 AI 人格，并可设置「用户身份」使对话体验更加贴合个人喜好。

---

## 📦 数据迁移与备份

GQY 提供便捷的打包与迁移指令，支持一键备份至 `.tar.gz` 文件（文件权限默认设为 `0600`）：

```bash
# 导出当前数据
gqy export                      # 导出基础配置、会话历史、记忆与知识库原文
gqy export --index --platforms  # 额外导出向量索引与外部平台聊天历史
gqy export --no-secrets         # 过滤 API Key 与令牌（推荐用于公开分享配置）
gqy export --dry-run            # 演练预览：仅检查打包清单与文件体积，不实际写入

# 在新设备上导入数据
gqy daemon stop                 # 导入前必须先停止正在占用数据库的 daemon
gqy import gqy-export-*.tar.gz  # 执行导入还原
```

> [!WARNING]
> 默认导出的归档文件中包含明文 API 密钥与令牌，请妥善保管归档文件，避免泄露至公开环境。

---

## 💖 致谢与鸣谢

### 功能与架构参考
- [shorin/miyu-agent](https://github.com/SHORiN-KiWATA/miyu-agent) - 核心基础与架构演进
- [Opencode](https://github.com/anomalyco/opencode)
- [Claude Code](https://github.com/anthropics/claude-code)
- [Pi](https://github.com/earendil-works/pi)
- [Deepseek-Reasonix](https://github.com/esengine/deepseek-reasonix)
- [Deepseek-Harness](https://github.com/deepseek-ai/deepseek-harness)
- [AstrBot](https://github.com/AstrBotDevs/AstrBot)
- [NapCatQQ](https://github.com/NapNeko/NapCatQQ)

### 插件与设计参考
- [Yue-bin/astrbot_plugin_maskoff](https://github.com/Yue-bin/astrbot_plugin_maskoff)
- [nuomicici/astrbot_plugin_GroupMemberQuery](https://github.com/nuomicici/astrbot_plugin_GroupMemberQuery)
- [advent259141/Astrbot_plugin_Heartflow](https://github.com/advent259141/Astrbot_plugin_Heartflow)
- [Railgun19457/astrbot_plugin_image_generation](https://github.com/Railgun19457/astrbot_plugin_image_generation)
- [xiewoc/astrbot_plugin_weather_wttr_in](https://github.com/xiewoc/astrbot_plugin_weather_wttr_in)
- [muyouzhi6/astrbot_plugin_recall_cancel](https://github.com/muyouzhi6/astrbot_plugin_recall_cancel)

---

## 📄 开源协议

本项目采用 [MIT License](LICENSE) 协议开源。
