# WebUI 拆分：架构设计

> 状态：P0–P5 已施工（2026-09-25），见 §11。与原计划不同之处也记在那里。
> 目标：把 `web/app.js`（13266 行）与 `web/styles.css`（11759 行）拆成有层次、有边界的模块，
> 并把「加一个文件」「加一个面板」变成不需要记忆隐性规则的事。拆分本身零行为变化。

## 1. 现状与病根

数字都是 2026-09-24 实测。

| 问题 | 现状 | 后果 |
|---|---|---|
| 上帝文件 | `app.js` 一个 IIFE 包 513 个函数；`styles.css` 按时间追加 | 找代码靠搜索，改一处不知道波及哪里 |
| 共享可变状态 | `state` 对象 125 个键、1106 处引用；`elements` 约 190 个 DOM 引用、736 处 | 任何函数都能改任何状态，拆文件前必须先定归属 |
| 事件集中挂载 | `bindEvents` 305 行，挂全部功能的监听 | 功能的「定义」和「接线」分在文件两头 |
| 公共层缺失 | 图标表 4 份（app/dashboards/shared/settings）、toast 3 份、API 封装 2 份另有裸 `fetch`、`formatTime` 3 份 | `shared.js` 注释原话：「拿不到那边的 createIcon，所以这里自带一份」 |
| 资源登记三处 | 每个文件要在 `src/web/mod.rs` 写常量、`assets.rs` 登记路由、`index_asset` 加 `?v=` 改写 | 拆成几十个文件时漏登记只会在浏览器里 404，编译照过 |
| CSS 覆盖散落 | 同一组件多处定义：assistant 在约 1800 行与 10800 行各一批，tool 在 3600 与 10800 | 层叠顺序就是行为，按组件归并会悄悄改样式 |
| 无门禁 | `refactor_size_report.py` 只统计 `.rs`；前端没有任何依赖方向或引用检查 | 拆完没有东西防止它再长回去 |

已有的好范例：`GqyDash`（`dashboards.js`）是一套小框架，提供 `register/api/openDrawer/pager/statCards`，
各 `dash-*.js` 只注册自己的面板。新架构把这种「内核 + 注册」模式推广到全站。

## 2. 目标结构

```
web/
  index.html
  app.js                  入口：只负责启动顺序（boot → 各 feature.init → bind）
  core/                   不依赖任何 feature
    api.js                唯一的请求封装（登录态、错误解析、JSON）
    icons.js              唯一的图标表 + createIcon
    dom.js                el()/清空/焦点等 DOM 小工具
    format.js             时间、字节、token、百分比
    prefs.js              safeStorage + UI 偏好读写
    toast.js              唯一的 toast
    ui-scale.js           zoom 与视口换算
  state/
    store.js              state 对象（按域分片，见 §4）
    elements.js           DOM 引用表
  features/               每个功能一个目录或文件，导出 init()/bind() 与少量对外函数
    sidebar/              会话列表、分组、拖拽排序
    model-menu/           模型与思考档位菜单
    composer/             输入框、附件、队列
    conversation/         消息渲染（用户/助手/推理/问题卡）
    markdown/             markdown 渲染与流式稳定化
    tools/                工具卡、命令输出预览、富卡片
    artifacts/            artifact 工作区、图片缩放、源码视图
    live/                 SSE 连接、run 事件分发、live 状态
    jobs/                 后台任务条、命令日志
    console/              用量统计与图表
    accounts/             账号、邀请、注册、登录
    oobe/                 WebUI 新手引导
    settings/             设置页（现 settings.js + settings-schema.js）
    dashboards/           现 GqyDash 内核 + dash-*.js
  widgets/                现有的独立小件：lightbox、preview、linkcards、diff、mapcard……
  css/                    见 §5
  vendor/                 不动
```

目录名是方向，不是施工清单：具体切分以第 3 节的规则为准，施工时按 `app.js` 的现有分区图落位。

## 3. 依赖规则（门禁强制）

层序：`core` → `state` → `widgets` → `features` → `app.js`。

1. 只能从左往右依赖：`core` 不 import 任何其他层，`state` 只 import `core`，依此类推。
2. feature 之间默认不互相 import。确有需要的边在 `test_scripts/web-deps.json` 里白名单声明（不放在 `web/` 下：那里的文件都会被当成静态资源发出去），带一句理由
   （例：`conversation → markdown`、`live → conversation`）。新增一条边要改这个文件，所以它会出现在 diff 里。
3. **模块顶层只声明，不执行**：不读 DOM、不调其他模块的函数、不引用其他模块的常量来算新值。
   副作用一律放进 `init()`。这条是 ES 模块循环依赖不出 TDZ 错误的前提：`const JOB_BRAILLE = BRAILLE_FRAMES`
   这类跨模块求值在循环里会直接抛错。
4. 文件规模沿用 Rust 的线：目标 800 行、上限 1500、红线 2000。

门禁脚本 `test_scripts/web_dep_check.py`：解析每个模块的 `import`，检查层序、白名单与引用路径存在；
`refactor_size_report.py` 扩到 `web/**/*.js` 与 `web/css/**/*.css`（排除 `vendor/`）。两者进 CI 的脚本门禁组。

## 4. 状态的归属

不引框架，不上响应式。只做一件事：**每个状态键有且只有一个主人**。

- `state` 拆成按域的分片：`state.sessions`、`state.live`、`state.artifacts`、`state.composer`……
  每个分片由对应 feature 的文件拥有，别的 feature 读可以，写必须调主人导出的函数。
- 第一阶段只搬家不改名：`store.js` 原样导出现在的 `state` 对象，保证零行为变化；
  分片改名放在该 feature 迁完之后单独提交。
- `elements` 同理：先原样搬进 `elements.js`，之后每个 feature 在 `init()` 里只取自己用的那几个。
- 现有的 `usageState`、`accountState`、`oobeState` 已经是分片的雏形，直接随各自 feature 迁走。

写入权的检查先靠约定与 review，不做运行时拦截；等分片改名完成后，门禁可以加一条
「`state.<域>.x =` 只能出现在该域的文件里」的文本检查。

## 5. CSS

1. **构建期拼接，不在运行时 `@import`**：`web/css/` 下按文件名前缀排序（`00-tokens.css`、`10-base.css`……），
   `build.rs` 按序拼成一份，仍从 `/styles.css` 提供。浏览器侧零变化，`/theme.css` 的覆盖顺序不变。
2. **第一刀严格保序**：按现有行序切成连续的段落，每段一个文件，拼回去必须和原文件逐字节相同
   （门禁里加一条字节对比测试，直到这一阶段结束）。
3. **第二刀才归并**：把散落的组件规则（assistant、tool 等）合到各自文件，每合一个组件单独提交，
   对照截图验收。这一步是唯一会改层叠顺序的地方，必须慢。
4. token 已经在 `00-tokens.css` 的范围内（颜色、字号、圆角、层级、时长），新规则禁止写字面量字号与 z-index，
   门禁用正则拦 `font-size: <数字>px` 与 `z-index: <两位数>`（白名单：已知的几何绑定项）。

## 6. 资源登记改为扫描目录

照 `src/tools/descriptions/*.json` 的先例：`build.rs` 扫 `web/`（排除 `vendor/`、`README.md`），生成一张
`(路径, include_str!/include_bytes!, content-type)` 表，路由按表统一分发，ETag 与 `no-cache` 策略不变。
`index.html` 的 `?v=构建号` 改写从逐条字符串替换改为统一正则替换所有同源 `src=`/`href=`。

加一个文件 = 把文件放进目录。另加一个 Rust 测试：`index.html` 与所有模块里出现的同源路径都必须在表里，
把「漏登记只在浏览器 404」变成编译期红灯。

vendor 的 gzip 预压缩资源（echarts、mermaid）和 KaTeX 字体保留现有的专门处理，不进扫描表。

## 7. 裁定（2026-09-24）

D1 原生 ES 模块 · D2 构建期拼接 · D3 本期 `app.js` + `styles.css` · D4 去重放进 P1。下表保留当时的比较。

| # | 问题 | 推荐 | 备选 |
|---|---|---|---|
| D1 | 模块机制 | **原生 ES 模块**（`<script type="module">`）：`app.js` 里的裸函数名换成 `import` 即可，搬迁最机械；依赖关系写在文件头，门禁能解析。浏览器全部支持，无需构建工具 | 保留 IIFE + `window.GqyXxx` 命名空间：和现有小件一致，但 513 个函数的相互调用都要改成 `App.xxx()`，改动面大、容易漏 |
| D2 | CSS 组织 | **构建期拼接**（§5.1） | 多个 `<link>`：零 Rust 改动，但要手工维护顺序，且多十几个请求 |
| D3 | 范围 | **本期：`app.js` + `styles.css` + 资源扫描 + 门禁**；`settings.js`（2680 行）排在下一期 | 一次连 settings 一起拆 |
| D4 | 公共层去重（4 份图标表等） | **放在迁移的第一阶段做**：core 立起来时顺手统一，否则各 feature 会继续各自复制 | 拆完再统一 |

## 8. 施工阶段

每阶段一个或几个提交，每阶段结束都能独立发布。

| 阶段 | 内容 | 行为变化 | 验收 |
|---|---|---|---|
| P0 基建 | build.rs 扫描 `web/`、index 改写统一化、路径完整性测试、`web_dep_check.py`、规模门禁扩到前端 | 无 | 全部页面资源 200；门禁在 CI 跑通 |
| P1 core | 建 `core/`，把四份图标表、三份 toast、API 封装、format 合一；旧的 `window.GqyXxx` 小件改为引用 core | 仅 toast/图标的细微样式统一 | 每个面板的 toast、图标逐一过目 |
| P2 app.js 迁移 | 先 `state`/`elements` 原样搬家；再按分区图逐个 feature 搬出，每个 feature 一个提交。配一个 `test_scripts/extract_js.py`（照 `extract_module.py`：按函数名搬、自动补 import/export、找不到名字就报错） | 无 | 每个提交后跑冒烟清单（§9） |
| P3 CSS 第一刀 | 按行序切段，字节对比测试守门 | 无 | 字节相同即通过 |
| P4 CSS 第二刀 | 按组件归并散落规则 | 可能有，逐组件确认 | 截图对比 |
| P5 状态分片 | `state` 按域改名、写入权收口 | 无 | 冒烟清单 |

## 9. 冒烟清单

仓库目前没有前端自动化测试，每个 P2 提交后手工过一遍（约 5 分钟）：

1. 登录 / 首次注册；深浅主题切换；侧栏开合与会话切换、拖拽排序。
2. 发一条消息：流式输出、推理块展开、工具卡、markdown（表格、代码、公式、mermaid）。
3. 中途停止；排队消息；提问卡的选择与自定义回答。
4. artifact：打开、最大化、图片缩放、源码视图。
5. 模型菜单与思考档位；设置页保存；任一看板打开抽屉。
6. 用量控制台图表；手机宽度（<836px）下重复 1–2。

后续可以考虑加一个无头浏览器的冒烟脚本，把这份清单自动化，不在本期范围。

## 10. 不做的事

- 不引入 npm、打包器、TypeScript、前端框架：资源编译进二进制、零构建步骤是现有定案，拆分不改变它。
- 不在拆分提交里顺手改行为。发现 bug 记下来，单独修。
- 不追求一次拆完。任何阶段停下来，仓库都处在比开始时更好的状态。

## 11. 施工记录

### P0 基建（2026-09-25）

- `build.rs` 的 `build_web_asset_index` 扫描 `web/` 生成 `WEB_ASSETS`；`src/web/embedded.rs` 逐条注册精确路由，
  并统一给 `index.html` 里的 `.js`/`.css` 引用挂 `?v=构建号`（图片不挂，JS 里按裸路径引用）。
  原来的 19 个手写 handler、19 条路由、`DASH_SCRIPTS` 表与对应常量删除。
- 看板与设置脚本的地址从 `/dash/<名>` 改为 `/<名>`（地址规则统一为「相对 `web/` 的路径」），只有 `index.html` 引用它们。
- 测试 `web::tests::embedded_assets`：页面引用与模块 import 必须解析到嵌入表；已验证把引用改回 `/dash/` 时报红。
- `test_scripts/web_dep_check.py` + `web-deps.json`：分层方向门禁，四条规则各自验证过报红。进 CI 与 `refactor-check.sh`。
- `refactor_size_report.py` 覆盖 `web/**/*.{js,css,html}`（排除 vendor），基线重写；拆分进度只算 `.rs`。

### 平台页与设置入口（2026-09-25，用户要求随拆分一起做）

- 控制台新增「平台」页：左侧平台列表，右侧分页。QQ 的「连接与设置」（原设置页 QQ 平台）、「消息记录」（原 `qq` 面板）、
  「群聊管理」（原 `groups` 面板）并到这里。加平台：`index.html` 加按钮与 `platform-body`，`features/console/panel.js`
  的 `PLATFORMS` 登记分页（`settingsPage` 复用设置页渲染器，`dash` 复用看板）。旧深链自动转到新位置。
- 保存栏只有一个：设置页或平台页的设置分页在显示时，`placeSettingsFooter` 把它挪过去。
- 左下角独立的设置图标删除，设置从控制台进（控制台是总入口，设置本来就是其中一页）。

### P2 app.js → ES 模块（先于 P1 做）

- 顺序对调：先拆，core 从 app.js 里自然分出来，再去合并旧脚本里的副本，比先凭空建 core 稳。
- `test_scripts/split_app_js.mjs`（acorn）按边界表机械搬运：原文与注释照搬，自动生成 import/export；
  顶层副作用语句收进各模块 `start()`，入口按原顺序调用。工具拒绝两类错误：跨模块给 `let` 赋值（ES 模块导入只读）、
  加载时读非 core/state 模块的值（循环依赖下的 TDZ）。
- 结果：66 个模块，最大 595 行；ESLint `no-undef` 前后均为 0。
- §3 规则 3「顶层只声明」的例外：`elements`（取 DOM 引用）与 `usageTip`（建元素）这类声明本身带 DOM 操作，保留原样。
- 功能之间的 116 条依赖是 app.js 闭包里本来就有的，原样冻结进 `test_scripts/web-deps.json`。缩减它们是后续工作。

### P1 公共层去重

- 图标表合一（106 个）：冲突时取旧脚本用的新版 lucide 图形；播放器的实心 play/pause 改名 `play-solid`/`pause-solid`。
- toast 合一：看板改用主提示条（看板只用到错误与普通两种）。看板 `api()` 走 `apiRequest`，登录过期同样回登录页。
- 三个 `formatTime` 不是重复：分别格式化时钟、日期时间、媒体时长，保留。
- `core/expose.js` 把公共层挂到 `window.GqyCore`，在 `index.html` 里排在所有旧脚本之前。

### P3 CSS 第一刀

- 36 个文件（最大 636 行），拼接结果与原 `styles.css` 逐字节相同（切分时脚本断言）。
  build.rs 按文件名顺序拼成 `/styles.css`，没有保留字节对比测试：P4 紧接着就要改动它。

### P4 CSS 归并：大部分不做（与原计划不同）

- `test_scripts/css_move_check.mjs` 逐条检查：挪动经过的每条规则里，有没有同优先级、同属性（含简写）的。
  主体类名前缀分属不相交源文件时视为不会命中同一元素，其余一律按可能命中处理。
- 结论：`82-bubble-overrides`（16 条里 0 条可挪）、`76-dashboard-extras`（54 条里 2 条）、`80-process-timeline` 是**真覆盖层**，
  靠「排在后面」生效，已在文件头标注，不挪。原计划「逐组件归并」的前提（散落规则只是写得乱）不成立。
- 只做了零风险的切点调整：`36-stage-states` 末尾的输入框规则并入 `40-composer`（拼接字节不变）。
- 真要继续归并，需要先有截图对比的视觉回归，再改选择器优先级，不能只靠挪位置。

### P5 状态分片

- 125 个共享键里 47 个只有一个模块读写，移入该模块的私有 `xxxState`；1 个无人使用的键删除。
  其余 77 个是多模块共用，或被 `settings.js` 经 `ctx.state` 读取，留在 `store.js`。

### 门禁

- `test_scripts/css_token_check.py`：每个 CSS 文件的字面量字号与两位数 z-index 只许减少（存量 50 与 7）。已进 CI。
