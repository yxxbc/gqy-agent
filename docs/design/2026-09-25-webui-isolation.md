# WebUI 与 gqy 的隔离：单独交付与运行时加载

> 状态：2026-09-25 裁定 D1=方案 A、D2=只在 debug 构建、D3 暂不做；方案 A 已施工，见 §6。
> 问题：WebUI 能不能单独「编译」、运行时动态加载，和 gqy 主程序大体隔离开？

## 1. 结论

- **源码层面已经基本隔离**。WebUI 是纯静态文件（无 npm、无打包、无转译），和后端之间只有一份 HTTP 契约。
  前端调用约 126 个不同的 `/api/*` 路径，`server.rs` 注册了 89 条 `/api` 路由（路径参数、查询串让两边计数口径不同）。
  Rust 侧直接读 `web/` 的只有 4 处，见 §2。
- **没有「单独编译」这回事**：前端没有编译步骤，所谓编译只是 build.rs 把文件嵌进二进制。真正耦合的是**交付方式**。
- 当前交付方式的代价是**改一行 CSS 也要整个 gqy 重编**：build.rs 对 `web/` 有 `rerun-if-changed`，每次重跑都用时间戳生成新的
  `GQY_BUILD_ID` 并以 `rustc-env` 传给编译器，环境变量一变整个 crate 失效重编（release 下 codegen-units=1，约 5.5 分钟）。
  9 月以来 `web/` 的文件改动次数是 `src/` 的约五分之一（507 比 2723），前端迭代时每次都要付这笔编译。
- **推荐：只做「开发期运行时目录」（§4 方案 A）**，发布包仍然嵌入。前端开发改完刷新浏览器即可，不重编；
  发布物的形态、安装路线和安全面都不变。「前端单独发布」（方案 B）收益很小、成本和风险都不低，不推荐。

## 2. 耦合点清单

| 耦合点 | 位置 | 性质 |
|---|---|---|
| 资源嵌入与路由 | `build.rs` `build_web_asset_index`、`src/web/embedded.rs` | 交付方式，可替换 |
| `index.html` 版本号改写 | `embedded.rs` `versioned_index` | 交付方式，可替换 |
| 沙箱宿主页 | `src/web/mod.rs` `FENCE_FRAME_HTML` + `assets.rs` 专用 CSP | 交付方式，CSP 必须保留 |
| vendor 预压缩库 | `src/web/mod.rs`（echarts、mermaid、KaTeX、Prism） | 交付方式；第三方库，基本不改 |
| 设置字段表与 Rust 默认值比对 | `src/web/tests/settings_schema.rs` 读 `web/settings-schema/` 拼成的 `/settings-schema.js` | **测试期**耦合，运行时无关 |
| 页面引用完整性 | `src/web/tests/embedded_assets.rs` | 测试期耦合 |
| HTTP API（JSON 形状、SSE 事件名） | `server.rs` 路由 ↔ `web/**` 调用 | **真正的契约**，任何方案都绕不开 |
| 主题覆盖 | `/theme.css` 读 `~/.gqy/config/webui-theme.css` | 已经是运行时加载（先例） |

API 契约没有版本号：`/api/health` 只报 `CARGO_PKG_VERSION`，bootstrap 不带协议版本。前后端同一次构建出厂，
所以今天不需要；一旦允许前后端版本错开（方案 B），就必须补。

## 3. 各方案

### 方案 A：开发期运行时目录（推荐）

daemon 启动时若设置了 `GQY_WEB_DIR=<仓库>/web`，静态资源改为每次请求从该目录读取，嵌入的那份不用。

- 读取规则与 build.rs 相同：同一套扫描排除规则，`css/` 按文件名现拼成 `/styles.css`，`index.html` 现做 `?v=` 改写
  （版本号取文件内容哈希），ETag 取内容哈希，`no-cache` 不变。`fence-frame.html` 的专用 CSP 照旧。
- 路径安全：只接受扫描清单里的相对路径，规范化后必须仍在目录内，拒绝符号链接逃逸。
- 效果：前端改完刷新浏览器即生效，不重编、不重启 daemon。Rust 改动照旧重编。
- 不影响：发布包、Nix、install.sh、daemon 按构建号重启的逻辑、所有测试（测试仍读仓库文件）。
- 工作量：约 150 行 Rust（抽出 build.rs 与运行时共用的扫描规则）+ 测试，半天。

### 方案 B：前端作为独立资源包发布

发布物里带 `share/gqy/web/`，主程序运行时从那里读（可保留嵌入作为兜底），前端可以不换二进制单独更新。

- 需要补 API 协议版本握手：前端启动先比对，不兼容就提示而不是半残运行。
- 打包三处都要改（`publish-release.yml`、`nix/package.nix`、`nix/prebuilt.nix`，AGENTS.md §7.3），install.sh 也要装这个目录。
- 收益很小：前后端同仓同版本出厂，几乎所有前端改动都伴随后端接口改动；单独更新前端的场景很少。
- 安全面变大：见 §4。
- 工作量：1–2 天，外加发布链验证。

### 方案 C：前端独立成项目（独立仓库、打包器、TypeScript）

与既有定案「不引入 npm、打包器、TypeScript，零构建步骤」（`docs/design/2026-09-24-webui-split.md` §10）冲突，
引入第二套构建链与依赖供应链。隔离得最彻底，但成本以周计，不推荐。

### 方案 D：把嵌入挪进独立的 workspace crate（排除）

设想是前端改动只重编一个小 crate。实际上 Cargo 在依赖重编后会连带重编所有依赖它的 crate，主 crate 照样整个重编，
拿不到好处。

### 插件自带页面（相关但不同的问题）

「动态加载」如果指第三方插件带自己的前端面板：2026-09-03 的看板方案已裁定「不走 iframe，不做插件页面发现机制」
（`docs/plan/2026-09-03-plugin-dashboard-and-io.md`）。若要重开，需要单独设计沙箱（iframe + postMessage + 独立 CSP），
与本稿无关。

## 4. 安全：运行时加载的前端就是管理员权限的代码

WebUI 登录后能跑工具、执行命令、改配置。**谁能写前端文件所在的目录，谁就能在管理员浏览器里执行任意代码**。
而顾清影自己有写文件的工具：如果运行时目录落在 `~/.gqy` 这类她能写到的地方，一次提示注入就可能改写前端、
把一次性的攻击变成常驻后门。

所以方案 A 的约束：

1. 只认环境变量 `GQY_WEB_DIR`，不进配置文件、不进 WebUI 设置（配置可以被工具改写，环境变量要人在终端里设）。
2. 生效时 daemon 日志与 `/api/health` 的 `web_assets` 字段明示「前端资源来自 <目录>」，避免忘了关。
3. 建议只在 debug 构建里生效（`cfg!(debug_assertions)`），发布构建直接忽略这个变量。代价是用 `cargo install`
   装出来的 release 版不能用它，前端开发要用 `cargo run`。这一条待裁定（§5 D2）。

方案 B 的 `share/gqy/web/` 在 Nix 下位于只读的 `/nix/store`，风险可控；在 install.sh 路线下是 `~/.local/share/gqy/web`，
与用户目录同权限，风险同上。

## 5. 待裁定

| # | 问题 | 推荐 |
|---|---|---|
| D1 | 做哪个方案 | **方案 A**；B、C 不做 |
| D2 | `GQY_WEB_DIR` 是否只在 debug 构建生效 | **只在 debug 构建生效**：发布版完全没有这条路径。备选：release 也认，但启动时打印醒目警告 |
| D3 | 构建号改成对输入内容取哈希（现在是时间戳，build.rs 每次重跑都换新） | **暂不做**：build.rs 只在被监视的文件变动时重跑，改成哈希只能省下「内容没变、只有修改时间变了」（例如切分支再切回来）的那次重编，收益小 |

## 6. 施工记录（方案 A）

- `src/web/asset_rules.rs`：哪些文件提供、什么类型、`css/` 怎么拼。build.rs 用 `include!` 引入，运行时作为模块使用，规则只有一份。
  所以文件里不能用 `//!` 内部文档注释（include! 到 build.rs 中间会报错），也只能用 std。
- `src/web/dev_assets.rs`（`#[cfg(debug_assertions)]`）：读 `GQY_WEB_DIR`，每次请求现读文件，`/styles.css` 现拼，
  `index.html` 现改写（版本号固定 `dev`），ETag 取内容哈希。目录里新加的文件由路由 fallback 提供（只接 GET/HEAD）。
  路径越界、`..`、特殊文件、跳过的目录、不认识的类型、指向目录外的符号链接一律 404，有单元测试。
- `embedded.rs`：各入口先问开发期目录，再用嵌入的那份；发布构建里这些入口恒为 None，设了变量只打一条「已忽略」的警告。
- `/api/health` 增加 `web_assets`：`embedded` 或 `dir:<路径>`。
