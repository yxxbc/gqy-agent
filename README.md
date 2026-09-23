<div align="center">

<img src="pics/gqy-logo.png" alt="GQY Logo" width="160" />

# GQY · 顾清影 Selene

**住在终端里的二次元 AI 伴侣：聊天陪伴、生活助手、写代码搭档，一个就够**

*Selene, a terminal-first anime AI companion for Linux and macOS.*

<p align="center">
  <a href="https://github.com/yxxbc/gqy-agent/releases/latest"><img src="https://img.shields.io/badge/version-0.6.0-blue.svg?style=flat" alt="Version"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-green.svg?style=flat" alt="License"></a>
  <img src="https://img.shields.io/badge/platform-Linux%20%7C%20macOS-informational.svg?style=flat" alt="Platform">
</p>

<img src="pics/gqy-tui.png" alt="GQY 终端界面截图" width="850" />

</div>

## 顾清影是谁

<img src="pics/gqy-mascot-is-not-the-tui-version..png" alt="GQY 吉祥物：戴白花发簪的像素黑猫" width="180" align="right" />

顾清影是一个常驻在你电脑里的 AI 角色。平时陪你聊天、记得你说过的事、帮你查天气汇率、定闹钟、记账；要写代码或排查问题时，切到开发模式，她就变成一个安静高效的编程助手。

> 「顾清影」最初是作者高中时期由 `Gemini-2.5-pro` 生成的虚构角色，现在她有了自己的家。英文名 Selene，取「起舞弄清影」月下清影的意象。

> [!NOTE]
> 这是一个业余维护的个人项目。遇到问题欢迎提 [issue](https://github.com/yxxbc/gqy-agent/issues)，我会尽力回复，但不保证时效。提问前先看看 [常见问题](docs/wiki/17-常见问题.md)，很多情况那里已经有答案。

## 她能做什么

- **两种模式，一键切换**
  - **普通模式**：有性格，有好感度和情绪。能闲聊、玩游戏、查天气汇率快递、看地图、生成图片、定时提醒。
  - **开发模式**（`gqy dev`）：收起人格和生活工具，只留写代码需要的东西，把模型的注意力全部用在你的项目上。
- **记得你**：长期记忆、日记、好感度与情绪。你纠正过她的事会带着理由记下来，下次不再犯；聊完一段时间后她还会自己复盘这次对话。
- **你的知识库**：把文档丢给她，在本机建索引，离线检索，内容不上传。
- **随处可聊**：终端界面、浏览器（手机平板也行）、直接在 shell 命令行里问、QQ、iMessage（macOS）。
- **本地语音**：喊一声「清影」唤醒，说完就办。语音识别在本机完成，不上传录音。
- **模型随便接**：
  - 兼容 OpenAI / Anthropic 协议的服务都能用。
  - 能借用你已有的 Claude Code、Codex、Antigravity 订阅。
  - 没有 API key 也能先用免费额度试。

## 安装

支持 **Linux**（x86_64 / ARM64）和 **macOS**（Apple 芯片 / Intel）。不支持 Windows。

### 方式一：Nix（推荐）

已经装了 [Nix](https://nixos.org/download/)（需要开启 flakes）的话，一行搞定：

```bash
nix profile install github:yxxbc/gqy-agent/gqy
```

- 下载的是编译好的程序，不用等本地编译，不需要 root。
- Intel Mac 不在 Nix 支持范围内，请用方式二。
- 只想先试试、不安装：`nix run github:yxxbc/gqy-agent/gqy`

### 方式二：一键安装脚本

没有 Nix，或者是 Intel Mac，复制这一行到终端运行：

```bash
curl -fsSL https://raw.githubusercontent.com/yxxbc/gqy-agent/gqy/install.sh | sh
```

- 装到 `~/.local`，不需要 root。字体、本地向量模型、默认知识库都一起装好。
- 安装时顶部是 GQY 字符 logo，底部是进度条（百分比、大小、速度）：

  <img src="pics/gqy-install-show.png" alt="一键安装脚本的安装界面：GQY 字符 logo、当前步骤、小贴士和进度条" width="720" />

  想先看看安装界面、不真的安装：`curl -fsSL https://raw.githubusercontent.com/yxxbc/gqy-agent/gqy/install.sh | sh -s -- --preview`
- 装完如果提示 `~/.local/bin` 不在 `PATH` 里，把 `export PATH="$HOME/.local/bin:$PATH"` 加到 `~/.zshrc` 或 `~/.bashrc`，然后重新打开终端。

<details>
<summary>手动下载安装包</summary>

在 [Releases](https://github.com/yxxbc/gqy-agent/releases/latest) 下载对应平台的压缩包，解压后把里面的 `bin`、`lib`、`share` 三个目录一起放到同一个位置（例如 `~/.local`）。三个目录要放在一起，程序会在旁边找资源文件。

| 平台 | 文件 |
| :-- | :-- |
| Linux x86_64 | `gqy-x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 | `gqy-aarch64-unknown-linux-gnu.tar.gz` |
| macOS Apple 芯片 | `gqy-aarch64-apple-darwin.tar.gz` |
| macOS Intel | `gqy-x86_64-apple-darwin.tar.gz` |

</details>

## 第一次使用

在终端输入：

```bash
gqy
```

第一次打开会进入新手引导，五步走完就能聊：

1. **选人格**：用内置的顾清影，或者自己捏一个。
2. **选功能**：勾选想要的插件（图库、地图、快递、记账……）。
3. **认识你**：告诉她怎么称呼你、你是做什么的、你们是什么关系。她对你的称呼和相处方式都按这里写的来，之后在设置的「用户身份」里随时能改。
4. **终端集成**：可选，让你在 shell 里也能直接问她。
5. **接模型**：
   - 可以借用已有的 Claude Code / Codex / Antigravity 订阅。
   - 可以用公共免费额度。
   - 也可以填你自己的 API key。

之后随时可以用 `gqy config` 修改这些设置。

> [!TIP]
> 推荐使用 [Kitty](https://sw.kovidgoyal.net/kitty/) 终端，图片能直接显示在对话里，体验最好。

## 日常使用

### 终端

```bash
gqy        # 普通模式
gqy dev    # 开发模式
```

- 空会话时按 `Tab` 在两种模式之间切换。
- 输入 `/` 可以看到所有命令，例如 `/new` 新会话、`/models` 换模型、`/config` 打开设置、`/help` 查看全部。
- 其他快捷键：`Shift+Enter` 换行，连按两次 `Esc` 打断她的回复，`Ctrl+D` 退出。

### 浏览器（WebUI）

```bash
gqy web
```

- 会打印一个局域网地址，同一 Wi-Fi 下的手机、平板也能打开。
- 第一次登录用内置账号（用户名 `gqy`，密码 `gqy`）。登录后会让你创建自己的管理员账号，建好后内置账号自动失效。
- 可以用邀请码请朋友注册，每个人的会话和记忆互相隔离。

### 直接在 shell 里问

```bash
gqy zsh-init     # 或 gqy bash-init / gqy fish-init
```

装好后不用进入对话界面，在命令行里就能直接和她说话。zsh 支持最完整。

### 语音

在 `gqy config` 里打开「语音功能」后：

- 喊唤醒词（默认「清影」「顾清影」），听到提示音后说出你的要求。
- 在终端里输入 `/stt`，或在网页上点麦克风，可以语音输入。
- `gqy listen` 可以绑定到桌面快捷键，按一下就开始听，不用喊唤醒词。

> [!NOTE]
> 预编译包暂时不带语音组件，需要语音的话目前要从源码编译，见 [参与开发](docs/wiki/14-参与开发.md)。详细说明见 [语音功能](docs/voice.md)。

### QQ 与 iMessage

顾清影可以接入 QQ（通过 NapCat）和 iMessage（仅 macOS），在手机上和她聊，也能拉进群里。设置方法见 [QQ 与通讯平台](docs/wiki/13-QQ与通讯平台.md)。

## 设置

以下三处改的是同一份配置，用哪个都行：

- 在终端运行 `gqy config`。
- 在对话界面里输入 `/config`。
- 在网页的设置页修改。

常用的几项：
- **供应商与模型**：推荐配置你自己的 API key，比公共免费额度更稳定。
- **自定义提示词**：创建你自己的人格，或者写一段「用户身份」让她更了解你。
- **插件**：随时开关各项功能。

## 升级与卸载

| 安装方式 | 升级 | 卸载 |
| :-- | :-- | :-- |
| Nix | `nix profile upgrade gqy-agent` | `nix profile remove gqy-agent` |
| 一键脚本 | 再运行一次安装命令 | 删除 `~/.local` 下的 `bin/gqy`、`lib/gqy`、`share/gqy`、`share/licenses/gqy` |

- 卸载前先运行 `gqy daemon stop` 停掉后台服务。
- 卸载只删程序，你的配置、会话和记忆都在 `~/.gqy`，要彻底清除就把它也删掉（建议先 `gqy export` 备份）。
- Nix 升级后有问题，可以用 `nix profile rollback` 退回上一版。
- 每个版本更新了什么，见 [更新日志](CHANGELOG.md)。

## 备份与换电脑

```bash
gqy export                      # 打包配置、会话、记忆和知识库
gqy export --no-secrets         # 不含 API key，适合分享给别人
```

在新电脑上：

```bash
gqy daemon stop                 # 先停掉后台服务
gqy import gqy-export-*.tar.gz  # 导入备份文件
```

> [!WARNING]
> 默认的备份文件里有你的 API key 等明文密钥，请妥善保管，不要上传到公开的地方。

## 更多文档

- [快速开始](docs/wiki/01-快速开始.md) · [功能总览](docs/wiki/02-功能总览.md) · [使用方式](docs/wiki/03-使用方式.md) · [命令参考](docs/wiki/04-命令参考.md) · [配置指南](docs/wiki/05-配置指南.md)
- [记忆系统](docs/wiki/08-记忆系统.md) · [安全与隐私](docs/wiki/16-安全与隐私.md) · [常见问题](docs/wiki/17-常见问题.md)

想自己编译、改默认人格或者参与开发，见 [参与开发](docs/wiki/14-参与开发.md) 和 [Nix 安装、开发与发布](docs/wiki/18-Nix安装开发与发布.md)。

## 致谢

本项目基于 [shorin/miyu-agent 0.6.0](https://github.com/SHORiN-KiWATA/miyu-agent) 深度重构与二次开发。

功能与架构参考：
[Opencode](https://github.com/anomalyco/opencode) ·
[Claude Code](https://github.com/anthropics/claude-code) ·
[Pi](https://github.com/earendil-works/pi) ·
[Deepseek-Reasonix](https://github.com/esengine/deepseek-reasonix) ·
[Deepseek-Harness](https://github.com/deepseek-ai/deepseek-harness) ·
[AstrBot](https://github.com/AstrBotDevs/AstrBot) ·
[NapCatQQ](https://github.com/NapNeko/NapCatQQ)

插件与设计参考：
[astrbot_plugin_maskoff](https://github.com/Yue-bin/astrbot_plugin_maskoff) ·
[astrbot_plugin_GroupMemberQuery](https://github.com/nuomicici/astrbot_plugin_GroupMemberQuery) ·
[Astrbot_plugin_Heartflow](https://github.com/advent259141/Astrbot_plugin_Heartflow) ·
[astrbot_plugin_image_generation](https://github.com/Railgun19457/astrbot_plugin_image_generation) ·
[astrbot_plugin_weather_wttr_in](https://github.com/xiewoc/astrbot_plugin_weather_wttr_in) ·
[astrbot_plugin_recall_cancel](https://github.com/muyouzhi6/astrbot_plugin_recall_cancel)

## 开源协议

[MIT License](LICENSE)
