# WebUI 供应商品牌图标（2026-09-24，待排期）

来由：用户问「WebUI 的供应商为什么全是字母，不是图标」。09-24 定：换成**彩色品牌图标**，先写方案，不施工。

---

## 一、现状与原因

项目里没有任何品牌图标素材，所有供应商都显示「名字前一两个字母 + 按名字哈希的底色」。两处独立实现：

| 位置 | 函数 | 用在哪 |
|---|---|---|
| `web/settings.js:171` | `mark(text, cls)`（`initials` + `hashHue`） | 设置页供应商卡片（`:1407`）、模型列表里每个模型前的小标（`:1897`，`is-small`）；人格头像缺失时（`:1955`）、插件卡片（`:2251`、`:2531`）也用它，这三处**不改** |
| `web/app.js:1719` | `modelMark(model)` | 聊天输入框的模型按钮（当前模型的缩写） |

当初这样做的理由仍然成立：供应商可以随意自定义，字母对任何名字都适用；WebUI 资源编译进二进制、不依赖网络。所以方案是**认得出的换图标，认不出的保留字母**。

## 二、图标来源与许可

- 来源：`@lobehub/icons-static-svg`（lobehub/lobe-icons，MIT）。彩色版文件名 `{id}-color.svg`，单色版 `{id}.svg`。
- **不走 CDN**：施工时把用到的 SVG 下载下来，内嵌进一个新文件 `web/provider-icons.js`（一个 `id → SVG 字符串` 的表），编译进二进制，离线可用。约 20 个图标，预计几十 KB。
- 许可：MIT，在 `web/provider-icons.js` 文件头注明来源、版本与许可；品牌标志归各公司所有，仅用于标识对应服务。打包资源若有 LICENSE 汇总处一并登记（查 `publish-release.yml` 与 `nix/package.nix` 的 licenses 步骤，AGENTS §7.3）。
- 版本钉死：记录取用的 `@lobehub/icons-static-svg` 版本号，以后要加图标从同一版本取。

## 三、要内嵌的图标

用户当前配置的 15 个供应商 + 常见的几家。「图标 id」一列按 lobe-icons 的 provider key 写，**施工时逐个核对 static-svg 包里文件是否存在、有没有 `-color` 变体**；没有彩色版的用单色版并让它跟随主题文字色（`fill="currentColor"`）。

| 供应商 | 图标 id | 备注 |
|---|---|---|
| Anthropic | `anthropic` | |
| Claude Code | `claude` | 借订阅的 CLI 通道，用 Claude 的图标 |
| OpenAI | `openai` | 通常只有单色版 |
| Codex | `codex` 或回落 `openai` | 核对是否有独立图标 |
| Gemini | `gemini` | |
| Antigravity | `antigravity` 或回落 `google` / `gemini` | 核对 |
| DeepSeek | `deepseek` | |
| 小米 MiMo | `xiaomimimo` | |
| MiniMax | `minimax` | |
| OpenRouter | `openrouter` | |
| Ollama | `ollama` | 单色 |
| LM Studio | `lmstudio` | |
| opencode Zen / OpenCode Go | `opencode`（若有） | 没有就保留字母 |
| 日日新 | `sensenova` | |
| 通义千问 | `qwen` | |
| Kimi / 月之暗面 | `moonshot`（或 `kimi`） | |
| 智谱 GLM | `zhipu` | |
| 豆包 / 火山引擎 | `doubao` / `volcengine` | |
| 硅基流动 | `siliconcloud` | |
| xAI / Grok | `xai` | |
| Mistral | `mistral` | |

## 四、匹配规则

新增 `providerIcon(provider)`，按顺序找，找到即停，都找不到返回 `null`（调用方回落到字母）：

1. **供应商 id**：`claude-code`、`codex`、`antigravity`、`openai`、`deepseek` 等与表里直接对得上的。
2. **类型字段**：`kind`/协议为 `claude-code`、`codex`、`antigravity`、`anthropic` 的。
3. **接口地址的域名**：`api.deepseek.com` → deepseek，`openrouter.ai` → openrouter，`generativelanguage.googleapis.com` → gemini，`xiaomimimo.com` → xiaomimimo，`minimaxi.com` / `minimax.io` → minimax，`opencode.ai` → opencode，`sensenova.cn` → sensenova，`dashscope.aliyuncs.com` → qwen，`moonshot.cn` → moonshot，`bigmodel.cn` → zhipu，`volces.com` → doubao，`siliconflow.cn` → siliconcloud，`x.ai` → xai，`mistral.ai` → mistral，`localhost:11434` → ollama，`localhost:1234` → lmstudio。
4. 都不中：`null`，保留现在的字母 + 哈希底色。

规则表放在 `web/provider-icons.js` 里，和图标同一个文件，加供应商只改这一处。前端拿不到接口地址的地方（模型列表只有 `provider_id`/`provider_name`），先按 id 匹配；需要的话在 `/api` 的模型条目里加一个后端算好的 `provider_icon` 字段——**施工时先看前端手里有什么字段再定**，能纯前端解决就不动后端。

## 五、改动点

| 文件 | 改什么 |
|---|---|
| `web/provider-icons.js`（新） | 图标表 + 匹配规则 + `providerIcon()` |
| `web/settings.js` | 供应商卡片（`:1407`）与模型列表小标（`:1897`）先调 `providerIcon()`，有图标就放 `<span class="st-mark is-icon">` 包 SVG，没有照旧 `mark()` |
| `web/app.js` | 输入框模型按钮：`modelMark()` 旁加图标分支，同样有图标用图标、没有用字母 |
| 对应 CSS | `.st-mark.is-icon`：去掉哈希底色，改成浅色圆角底板，SVG 居中 16–20px；深色主题底板换深一档，保证彩色图标和单色图标（`currentColor`）在两种主题下都清楚 |
| `web/index.html` | 引入 `provider-icons.js`（在 `settings.js` / `app.js` 之前） |
| 静态资源清单 | 若 WebUI 资源由 `build.rs` 或资源表逐个登记，同步加上新文件（AGENTS §7.1：web 静态资源编译进二进制） |

TUI 不改（画不了 SVG）。

## 六、测试与验收

- 单元层：WebUI 没有 JS 测试框架，匹配规则写成纯函数，施工时评估能否复用 `src/web/tests/settings_schema.rs` 那种「Rust 读 JS 字面量」的方式，至少断言：规则表里每个 id 在图标表里都有对应 SVG；每个 SVG 字符串是合法的 `<svg …>…</svg>`。
- 截图：用浏览器分别在浅色、深色主题下截设置页的供应商列表和输入框模型按钮。
- 验收（给用户）：
  1. 设置 → 供应商和模型：15 个供应商里认得出的都显示品牌图标，自定义且认不出的仍是字母。
  2. 聊天输入框的模型按钮显示当前模型所属供应商的图标。
  3. 切换深浅主题，图标都看得清。
  4. 断网打开 WebUI，图标照常显示（确认没走 CDN）。
