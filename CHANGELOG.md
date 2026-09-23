# 更新日志

格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

写法约定（`python3 .github/scripts/release.py check` 会检查，CI 也跑）：

- 新改动写进 `## [Unreleased]`，验收通过后再写。发版时工作流把它改成 `## [X.Y.Z] - 日期`，这一段原样成为 GitHub Release 的正文。
- 版本标题下可以先写一段概述，然后只用这几个小节，按此顺序：`### Added`（新增）、`### Changed`（变更）、`### Deprecated`（即将移除）、`### Removed`（移除）、`### Fixed`（修复）、`### Security`（安全）。
- 每条以 `- ` 开头，写给用户看：说清改了什么、对用户有什么影响。纯内部重构、文档、CI 不写。破坏性变更在条目开头标 **破坏性**。

## [Unreleased]

本版本起项目对外改名为 GQY / 顾清影，顾清影成为内置默认人格。安装以 Nix 为主，没有 Nix 的机器用一键安装脚本。

### Added

- 内置图库（`album`）、地图（`map`，开源数据优先，高德可选）、快递 100 查询（`express`）三个插件，WebUI 配套图库面板、地图卡片和快递时间线卡片
- WebUI：上下文占用分项拆解面板、划词助手、围栏图表（Mermaid）预览，成果交付增强
- 新增 `github` 工具：commit、提 PR / issue、评论与 gh 直通。你固定是 author，顾清影自动挂 `Co-Authored-By`；名字与邮箱在 `tools.github` 配置，`gqy reload` 生效
- 顾清影可以有自己的 GitHub 账号：`gqy github login / status / logout`，凭据隔离在 `~/.gqy/github/`，不碰你的 gh / git 登录与钥匙串。默认用你的身份，明确要求时才用她的账号，推不进的仓库自动 fork 后提 PR
- iMessage 本地守护桥接（macOS）
- 纠正型记忆：你纠正过的事情会带着理由记下来，优先级最高
- 日常聊天复盘：会话空闲一段时间后顾清影回顾这段对话，最多记 3 条改进要点，下次聊天时参考。时长由 `plugins.memory.review_idle_seconds` 控制（默认 900 秒，0 关闭）。WebUI 记忆页新增「复盘」页签，只读
- Antigravity 预热进程的保留时长可配置：设置里「Antigravity 中转 → 预热进程保留时长」（`plugins.antigravity.warm_idle_seconds`），设成 0 完全关闭预热
- TUI：输入框为空时显示淡色提示，文案与 WebUI 人格看板的输入框提示同源（默认「给 <人格名> 发消息」），开发模式不显示
- TUI 底栏的上下文占用改成占用条 `47k/168k ▰▱▱▱▱ 28%`，占用低于 60% 绿色、低于 85% 黄色、更高红色。终端窄时先退成纯百分比
- 新配置项 `display.theme`（`auto` / `dark` / `light`，默认 `auto`）：终端底色深浅。自动模式下交互 REPL 启动时向终端查询一次底色。环境变量 `GQY_THEME` 可临时覆盖。WebUI 设置页和 `/config` 都能改
- 顾清影有了英文名 Selene（取「起舞弄清影」月下清影的意象）。英文界面里指她本人的提示改用 Selene，例如 `Selene is listening`、`Selene finished replying`
- 内置人格补了 4 段示例对话（日常、排障、被纠正、问模型），语气更稳定
- 生图失败次数有了上限：同一轮里失败满 5 次就不再重试，顾清影会把出错原因告诉你。本地对话和 WebUI 也生效，原来只有通讯平台有配额
- 一键安装脚本有了安装界面：顶部是带扫光的 GQY 字符 logo，下面是当前步骤与小贴士，底部是进度条（百分比、已下载 / 总大小、速度），整块原地刷新。`sh -s -- --preview`（或 `GQY_PREVIEW=1`）只播放界面、不联网也不安装。不是终端、终端窄于 60 列或设置了 `GQY_PLAIN=1` 时仍是纯文字输出

### Changed

- **破坏性**：项目全面改名 Miyu → GQY / 顾清影，顾清影成为内置默认人格，logo 改为 GQY
- **破坏性**：内置顾清影人格改为面向所有用户的版本。她和你是什么关系、怎么称呼你，改为按用户资料（新手引导「认识你」，或设置里的「用户身份」）来；资料里写明是恋人时才会撒娇、说情话，否则就是熟络的朋友，也不再预设你的性别。喜欢原来设定的话，在用户资料里写明你们的关系和你希望的称呼即可
- 内置知识库改为来自项目仓库的 `kb/` 目录，`gqy update-default-kb` 与网页端「更新」都从这里拉；只有 `kb/` 里的内容变了才会提示更新，没有随包快照的安装方式（如 `cargo install`）也能直接更新
- 删文件时优先移进回收站：命令工具的说明会引导她用「移入回收站」而不是 `rm`，删错了能找回
- 回答时不会再声称查过、跑过实际没做的事：任何人格都带上一段通用的诚实规则（工具失败就说失败，不确定就说不确定）
- 安装以 Nix 为主（`nix profile install github:yxxbc/gqy-agent/gqy`），没有 Nix 或 Intel Mac 用 `install.sh` 一键安装，预编译包由云端构建
- 一键安装完成后的提示改为直接运行 `gqy`：第一次打开会进入新手引导，不再需要先 `gqy init` 和 `gqy daemon start`
- Antigravity 每轮少等约 6 秒：一轮结束后立刻把下一轮要用的 agy 进程拉起来备着，第二轮起基本只剩模型生成时间。只有常驻 daemon 会预热，只备一个，默认 5 分钟没人用自动关闭。换会话、换发起来源、换续传目标时不复用，预热进程意外退出会自动退回冷启动
- 全屏 TUI 默认开启
- TUI 配色：界面色（输入框竖条、模式标签、选中标记、成功 / 警告 / 错误色）跟随终端的 16 色配色方案；代码高亮、diff、展开区底色按深浅底各备一套，浅色终端下也看得清
- TUI 思考过程改为暗色斜体（原为亮绿），等待动画改用主色
- 顾清影说话不再过短：默认人格提醒去掉「整条不超过一百字」的硬上限，篇幅跟着场合走
- WebUI 地图卡片默认收进工具签的折叠区，不直接展示位置

### Removed

- 上游继承的 Arch / DEB / RPM 打包。安装只保留 Nix 与 `install.sh` 两条路线
- 内置知识库里上游作者的 Arch Linux 指南。更新一次内置知识库后，本地这部分文档会被移除，你自己加的文档不受影响

### Fixed

- macOS 麦克风：48kHz 重采样加抗混叠、优先使用 16kHz、ALSA 过滤只在 Linux 生效，修复 KWS 唤醒词不命中
- macOS 上嵌入与渲染 worker 因内存上限（RLIMIT_AS）起不来
- macOS 上 TUI 点击链接只提示「正在打开链接」却没有打开；打不开时改为提示「无法打开链接」
- macOS / Safari 的 WebUI 体验问题与 4 个内置脚本不稳定
- 引导「自选功能」里图库、地图、快递三个插件没有名字
- 终端配置器的全局设置、插件设置、QQ 菜单改为按字段绑定写回，插入新设置项不会再把值写进错误的设置
- 纯中文人格名的数据目录迁移不完整
- fish 的 shell hook 写到了 fish 不读取的配置目录

### Security

- bot 身份下 gh 找不到自己的 token 时，不再回退使用宿主钥匙串里的 token：没有 bot token 一律拒绝执行
- 后台任务与会话的多用户访问隔离收口
- Landlock 沙盒只在 Linux 上启用，其他平台明确拒绝而不是假装已隔离

<!-- legacy: 以下为 git-cliff 生成的历史记录，保留原格式，release.py check 不检查 -->

## [0.6.0] - 2026-09-13

### 🚀 Features

- *(memory)* Reset-memory 只清本会话, 新增 reset-all-memory；wipe 不再删技能
- *(sponsor)* 通讯平台会话的赞助记账
- *(webui)* 链接卡片 / 附件预览 / 表情包瀑布流 / 芯片截断 / 赞助面板
- *(webui)* 自己发的消息渲染代码块与链接, 附件图标按类型分
- *(webui)* 代码块语法高亮
- *(webui)* 控制台位置写进 URL hash,设置页宽屏居中
- *(qq)* 撤回日志写清撤的是谁的消息
- *(compact)* Compact v3 —— 压后重建、折叠原文回查、摘要结构升级、用量锚点
- *(scripts)* 内置 Reddit 检索工具 reddit_search
- *(render,webui)* 标题也算链接,链接色固定成蓝,代码块去描边
- *(dev)* 开发模式提示词瘦身——技能与记忆整套退场
- 上键历史活占位符/Safari 流式抖动/tok·s/记忆整理/shellhook 提问
- *(tools)* 扩展清单五字段——脚本头部 Trust/Permission/Example/Hint/Requires + 图片回传
- *(prompt)* 风格锁给外部受众(阶段 2 部分)+ 计划勾选与待并入 release note
- *(persona)* Persona.toml 清单 + 三表合一 compose_registry + 场所信任位(阶段 4 前半)
- *(web)* 多用户(邀请制账号、会话归属、管理台闸、用量按人)——分层架构阶段 5
- *(paths)* 家目录布局 personas/ extensions/ home/<用户>/ + miyu layout——分层架构阶段 6
- *(pm)* 包管理器 miyu pm(install/remove/upgrade/search/list/tap,miyupm shim)——分层架构阶段 7
- *(web)* 三步引导 + 成员私有人格;修字跳/跨会话拦截/脚本显示名/独立登录页——沙盒试用反馈
- *(ledger)* 分类现建/账户余额折算/默认账户/预算余额卡
- *(qq)* 睡眠时间——时段内只有管理员与私聊白名单的消息叫得醒
- *(webui)* 过程时间线——思考与工具串成一条细线,AI 去气泡,用户气泡改中性色
- *(webui)* 时间线耗时落库,刷新不丢;去气泡后正文里的面重新定色;准备签不蹦、快模型出行错开
- *(webui)* 思考内容收着时,思考的尾巴放在「已思考」那一行里
- *(webui)* 排队的消息直接画在对话末尾,左边一枚「排队中」小签
- *(multi-user)* 首次访问建管理员(内置口令 miyu,-p 退场)+ 成员 Landlock 沙盒 + 账号页样式 + 合 main
- *(webui)* Artifact 面板放开脚本沙箱 + 内置 ECharts;svg/csv/源码高亮补齐
- *(auth)* 首次访问的内置登录改成账号 miyu / 密码 miyu
- *(subagent)* 后台子代理带作用域与收件箱 / vision 走旁路 / follow-up 工具
- *(tools)* 子代理工具改名 subagent,并允许开开发模式
- *(subagent)* 后台子代理实时子过程流——任务行可点开看流式渲染
- *(webui)* 输入框重排——框内只留交互按钮,模型/tok·s/累计/上下文移到框下一行(#99)
- *(webui)* 输入框三行 + 附件移左下角 + 语音/发送合并同位
- *(webui)* 子代理 prompt tag 并入时间线开头(无背景/悬浮换 logo)
- *(webui)* 子代理正文流式渲进子过程时间线(#6 光有 timeline 没正文)
- *(webui)* 运行中进度条(三点)移到附件按钮右侧(用户要求)
- *(webui)* 刷新后回放后台子代理子过程(#9 刷新丢内容)
- *(webui)* 输入框加载指示改成编排点动效(用户拍板)
- *(webui)* 编排点动效改用在 AI 输出「加载中」,输入框恢复旧三点(用户改主意)
- *(webui)* 文件编辑工具渲染 diff(而不是原始补丁 JSON)
- *(webui)* 输入框「累计」边跑边涨——含子代理实时汇入(#131)
- *(webui)* 模型 effort 成员可自配,不再 admin only (#162/#163)
- *(sandbox)* /sandbox 会话级 Landlock 沙盒,/workspace 退役
- *(tui)* 全屏 TUI + 时间线式过程渲染(shellhook 静态版、子代理浮层、回放)
- *(tui)* 命令尾巴六行且跑完保留、全屏链接、shellhook 跟进走时间线、转轮同列、面板正文 markdown
- *(oobe)* 新手引导、空会话大厅、MCP/技能人格闸

### 🐛 Bug Fixes

- *(memes)* 识图返回带围栏或前言也能入库
- *(kb)* 上传闸门只认路径与技能标记, 不再扫正文关键词
- *(kb)* 重建请求不再蒸发, 加进度与失败可见; 面板批量选与观感修正
- *(kb)* 落盘位置由 name 现算; 补齐面板图标表; 批量条精简
- *(kb)* 重建进度不再提前显示 100%
- *(webui)* 统计卡小字不再撑破卡片; 重建百分比按真实工作量算; 气泡内代码块配色
- *(webui)* 语法高亮改用标准色板(One Dark / One Light)
- *(webui)* 浅色代码面板改用页面自己的纸色
- *(test)* Refactor-check 的路径修回 test_scripts/
- *(webui)* 插件抽屉标签条不再被长内容挤扁; 手机 Safari 键盘不再顶走整页
- *(renderer)* 表格列宽按内容分配; 链接与图片保留地址; paper 链接色离开暖色系
- *(qq)* 好感度查询工具瘦身; 档位改英文规范名; 护栏从死代码移进工具描述
- *(web)* 目标续轮跑着时 WebUI 发消息不再 409
- *(usage)* 供应商改 id 后用量账本跟着改名
- *(usage)* 摘要轮的缓存命中不再落库记 0
- *(compact)* 压缩不再拖垮其他会话,也不再必然超时
- *(compact)* 回灌与 footprint 认出 edit 补丁头，压缩套会话模型池
- Todolist Main 六项——回车光标拖尾/粘贴占位符/shellhook 失忆/链接卡片/生图复读/撤回内容
- *(compact)* 中转线统一走 Miyu 压缩——remote 轮进 footprint 与回灌
- *(webui)* 输出速度写成「每秒 x toks」
- 终端逐 delta 成段/手机端回车换行/用量页竖排/回到底部偏移/上下文窗口按会话/重建保跟随
- *(repl,webui)* /models 结果行不再压到输入框上;会话内换模型后立刻重拉上下文条
- *(webui)* 删最后一个会话时顶替会话改由服务端建,多端同开不再冒出两个
- *(repl)* /models 换模型后 footer 模型标签立刻重绘
- *(webui)* 时间线上思考中的节点保持原子图标,「准备 xx」签改成时间线上的一行
- *(webui)* 快模型的工具行合并出现而不是排队;表格阴影打在滚动容器上;行内代码 8px 圆角
- *(webui)* 命令签收起态不再有输出预览气泡;展开面板里流式输出与最终结果不再重复
- *(webui)* 时间线开合时线逐帧跟着裁剪边走,不再拖在内容后面;「展开思考内容」默认关回去
- *(webui)* 思考窥视放得下就左对齐,放不下才尾部可见
- *(webui)* 思考行的展开箭头跟着文字走,不再被窥视槽推到最右边
- *(webui)* 工具行没有主语就空着,不再写「无输出 / 等待输出」
- *(multi-user)* 成员各自一份会话库 + 引导页措辞/插件语义/脚本逐个勾选/人格生效/成员 dashboard
- *(multi-user)* 会话命令(改名/排序/删除/工作区/模型覆盖)按会话找库
- *(multi-user)* 引导页文案/去看板图、成员面板按人格、表情包按人分库、工具桥按人格、换模型 500
- *(webui)* 排队小签的按钮换成撤回图标;词内下划线不再渲染成斜体
- *(webui)* 发出消息后回到底部
- *(sandbox,assets)* 成员沙盒外读也禁 + 进程内工具路径守卫 + 图片/artifact 资源按登录者的库取
- *(webui,agent)* 命令登录后重拿、成员可开 dev 会话、表情包提醒看工具面、host-environment 带模型/档位、几处 UI 小修
- *(webui,render)* 「标题 (地址)」整行成链只认标题不认段落;手机端到底后再拖输入框被推出屏幕
- *(sandbox,auth,webui)* 中转线沙盒回合关原生工具、登录态落盘 30 天、成员排队、输出速度落库、host-environment 沙盒/mixed
- *(sandbox,prompt)* 中转线 CLI 进程整个关进沙盒(原生工具照开);WebUI 回合也带 host-environment
- *(agent)* 发请求前配平 tool_calls/tool 结果防 400;host-environment 去掉 effort
- *(webui)* 桌面端底部露出一条空带
- *(kb)* 成员的知识库语义索引重建落到自己的库
- *(webui,artifact)* 成员 artifact 落自家目录 + PNG 预览 + 缓存百分比口径 + 任务列表浮层 + 子代理工具行改单行窥视
- *(relay,config)* 中转线说明与注释跟上 subagent 改名
- *(llm,web,subagent)* 多模型池按池均衡 / 成员看得到后台任务 / 成员 CLI 后端 MCP 桥可用 / 子代理 WebUI 全量流
- *(subagent,goal,webui)* 子过程思考累加 / 移除 goal 轮数限制 / 开发子代理标「开发中」/ goal 条移到后台任务下
- *(subagent)* 子代理工具行认出 subagent:<描述> 事件名 + 后台开发子代理标「开发中」
- *(llm)* Opencode Console Go 端点补上 Zen 识别头
- *(qq)* 解禁之后不再一直自认被禁言 / 静默跳过补留痕
- *(goal+webui)* Goal 工具/驱动器用会话属主库 + 任务条重做(braille/窥视/token/命令展开)
- *(webui+subagent)* Todolist main-fixes 一批(附件预览/排队/编辑prompt/切会话/goal框/断点续传落盘/artifact缩放)
- *(subagent+webui+mcp)* 前台子代理读秒/token · 命令输出 · 中转 MCP 沙盒 exec · followup 微字 · 手机排队
- *(mcp+webui)* 成员中转 MCP 桥连不上真凶坐实 + 子代理/任务条/输入框一批
- *(webui)* 子代理/任务条渲染一批(空心spinner/工具友好名/思考空格/后台brief/窥视不压标题…)
- *(webui+terminal)* 子过程工具卡动画 / 打印图刷新不跳尾部 / xterm chafa 探测
- *(webui)* 切回前台只在 SSE 断过时才重建会话(修长对话滚动卡死诱因 #13)
- *(webui)* 展开收起消抖(#3) + 状态行整行可点(#11验证) + 登录时不重复弹会话重载(#18b)
- *(webui)* 思考节点换灯泡图标(#10) + 中转线工具tag出现平滑不错位(#17)
- *(webui)* 子代理状态行整行可点(#11) + 收起时重置内部展开(#5)
- *(webui)* 子代理/时间线精修一批(灯泡回退atom/整行可点hover/展开动画/sticky真修等)
- *(alarm+sandbox)* 闹钟认自然语时长 + 成员人格脚本放行执行
- *(webui)* 子代理精修二批(动画重做/sticky真修/窥视布局/崩溃止血) + followup排队同源
- *(webui)* 输入框停止按钮内联/加高/手机显模型 + 子代理四行活区域 + 节点背景匹配容器
- *(webui)* 输入框按钮重设计(圆形发送+幽灵副键) + 子代理 prompt 折叠成可展开 tag
- *(webui)* 刷新后后台任务状态行消失(补拉时错把 Response 当 JSON)
- *(webui)* 任务行窥视改左对齐(#2/#3) + 输入框两行 + 停止按钮浮发送键上方
- *(webui)* 子代理正文代码块/表格亮色下白底改暖底(#9)
- *(webui)* 后台任务行悬浮加选中高亮(#5)
- Send_subagent_message 缺友好显示名(#用户报)
- *(webui)* 子代理空 reasoning/content delta 造空块切断时间线(用户报「串」)
- *(webui)* 子代理面板一批(#1-#2/#4-#8/#10)
- *(tools)* Search_web_images 成员场景存到成员家而非管理员家(#11)
- *(webui)* 子代理面板二批(#1/#2/#4/#5/#7 + 吸顶重叠)
- *(webui)* 回归修复+配色统一(#121/#123/#129/#130/#122/#3)
- *(webui)* 窥视淡出仅溢出时/后台面板缩进与双框/后台命令窥视(#132/#133/#134/#120)
- *(web)* /reset-memory 不再 admin only,按会话作用域清发命令者自己的记忆(#137)
- *(webui)* 面板去左缩进/合并行显箭头/贴底跟随重做(#1/#138/#139)
- *(webui)* 面板左缩进 8px/去问答大对钩/流光 demo 更新(#2/#4/#1)
- *(webui)* 编辑人格不再走 onboarding 的庆祝收尾(#146)
- *(webui)* 前台/已完成子代理刷新后子过程时间线不再消失
- *(webui)* 前台子代理运行中刷新不丢子过程 + 加载动画调小 + 后台子代理限高
- *(webui)* 刷新时前台子代理不再鬼影——重连给 live 气泡播种
- *(webui)* /reset-all-memory 带上 session_id(成员不再报 session not found)
- *(webui)* 累计涵盖后台子代理 + 修后台跑完掉数 + 子过程刷新自动滚(#131/#159)
- *(webui)* 子过程加块贴底滚 + 去问答大对钩 + 新建人格加取消(#160/#161/#164)
- *(webui)* 中断秒停/artifact 不盖页脚/去竖排等待回答/刷新问答卡归位
- *(webui)* 中断少一次重渲染 + artifact 页脚 z-index 兜底/开面板即量
- *(webui)* 已回答问答卡列表左对齐(#166)
- *(terminal)* 终端图片四修 —— 旧版 chafa 崩、小图放大、放不下丢图、kitty 环境泄漏
- *(tui)* 子代理浮层转轮、收缩行展开成时间线、窥视不再是裸 JSON、不报 0.0s
- *(tui)* 浮层「运行中」行终于会出现、秒数会走；收缩行合 › 开 ⌄；后台面板去 ok、展开正文改主题
- *(config)* 比本版本新的配置读得进、不降级，陌生字段原样保留
- *(tui)* 面板正文按面板宽度排、日志正文续行不丢、后台转轮按时间定帧；准备行/Arch 工具图标；防失忆两项挪进普通模式
- *(tui)* 准备行转轮不再鬼畜；全屏 /new 清屏并回放目标会话；斜杠命令期间鼠标上报不再回显成 ^[[<35;…M
- *(tui)* 全屏压缩上下文写进正文并收成摘要块；后台状态行转轮按时间定帧

### 📚 Documentation

- 09-09 第三轮验收记录
- 划掉记账、归档 CLI 重做计划
- 分层架构施工计划(core/扩展/persona/场所、家目录、多用户、可删项清单)
- *(plan)* AgentMode 现状标注为派生标签,机械替换另提交
- Next-release-note 记 WebUI 过程时间线/去气泡/自动收起/去时间
- Release note 补时间线耗时落库/正文面定色/文案不假定性别
- Release note 补排队消息入时间线 / 思考窥视
- Release note 补词内下划线不斜体
- 新增 as-built 架构总结 docs/architecture.md
- *(architecture)* 核心工具清单跟上 subagent 改名
- Todolist 重排 / 两份专项计划入库 / 发版手册 / release note 追加

### 🚜 Refactor

- *(tools)* Goal 三合一、搜索要求列来源、跨工具指路句随注册表走
- *(tools)* 十件日用工具迁成内置脚本,Rust 实现退场(阶段 3)

### 🎨 Styling

- *(webui)* Diff 卡去描边 + 换干净配色
- *(webui)* Diff 卡加柔和阴影(阴影代替描边抬起卡片)
- *(webui)* Diff 卡阴影收小一档

### 🧪 Testing

- 清掉误入库的沙箱锁文件并 gitignore
- A 组断掉工具, 否则暗号判据是假阳性
- WebUI 与知识库拖拽的真机走查测具, 09-09 验收记录
- *(compact)* 压缩质量 A/B 测具 testkit/compact-quality
- *(tools)* 三张注册表形状指纹夹具(阶段 4 安全网)
- *(scripts-migration)* 探针隔离 XDG_RUNTIME_DIR(否则桥连真 daemon)+ persona.toml 端到端探针
- *(tui-demo)* TUI 重写演示与抓屏测具
- *(tui)* Round26 切回旧会话按 2 号再试 1 号（/new 之后新会话是 1 号）
- *(tui)* Round26 新增 /compact 场景（start 支持盖配置：尾巴预算压到 40 词元）

### ⚙️ Miscellaneous Tasks

- 上游仓库改名 SHORiN-KiWATA/Miyu → miyu-agent(packaging/Cargo.toml/README)
- *(build)* 编译并行度按住 3 个核心
- *(testkit)* 删掉 chafa 量尺里两条走不通的路

### 💼 Other

- 0.5.0 资产 sha256
- Miyu-git 快照 0.5.0.r794
- 中转线补「准备xx」提示(RemoteToolPreparing)
- 拦下 Google 内容策略文案,不再当回复漏到 QQ
- 0.5.0-2 补丁资产(claude-code 准备提示 + agy 策略文案拦截)
- Kitty 图片滚动残影根因坐实——发过图后腾地方改整屏滚
- 程序驱动 CLI 体系——回合级覆盖、json/stream-json 输出、stdio 长驻、session 管理面
- 补两处 rustfmt 遗漏
- 机械折叠层整层退役，新增 miyu compact
- 手动压缩改成流式，摘要边生成边暗色打出来
- 划掉已发布项
- 模型列表支持手填模型名，n 添加、置顶标自定义
- 发往 opencode Zen 的请求补上客户端识别头
- Zen 会话头按 Miyu 会话走，不再整个 daemon 共用一个
- 中间正文改在工具边界发, 不再攒到回合末尾
- 中间正文按工具边界发 + PDF 输入通路
- 输入框提示做成人格可配, 默认跟着人格名走
- WebUI 输入框提示可配
- 数据层与工具面
- WebUI 面板
- 文档与手机走查清单
- 09-09 十四项优化 + WebUI 语法高亮
- 长文转图表格/链接渲染 + WebUI 抽屉与手机键盘 + 好感度工具瘦身
- 记账
- 圆角收敛成三个变量
- WebUI 圆角统一
- 09-09 五项修复
- Compact v3(压后重建/折叠回查/摘要 v3/用量锚点)
- 内置 Reddit 检索工具(reddit_search, Arctic Shift 归档)
- Compact v3 实况修复(压缩不再拖垮其他会话/不再必然超时)
- Compact 回灌名单修复 + 压缩套会话模型池
- Todolist Main 六项修复(回车光标拖尾/粘贴占位符/shellhook 失忆/链接卡片/生图复读/撤回内容)
- 开发模式提示词瘦身(技能/记忆退场,工具目录 −23%)
- 中转线统一走 Miyu 压缩(remote 轮进 footprint 与回灌,不再传 --autocompact)
- 上键历史活占位符/Safari 流式抖动/tok·s/记忆整理/shellhook 提问
- WebUI 输出速度写成「每秒 x toks」
- 终端逐 delta 成段/手机端四项/上下文窗口按会话/REPL /models 布局 + 分层架构施工计划
- 记账分类现建/账户余额折算/默认账户/预算余额卡
- WebUI 过程时间线 + 去气泡 + 过程自动收起设置
- 时间线思考节点保持原子图标 + 准备签成行
- 时间线耗时落库 + 正文面定色 + 准备签/快模型出行
- 工具行合并出现 + 表格阴影/行内代码圆角 + 思考默认展开 + 点击判定收窄
- 命令签去输出预览气泡 + 展开面板去重
- 思考收着时尾巴放在已思考那一行
- 时间线开合跟裁剪边同步 + 展开思考内容默认关
- 思考窥视对齐 + 状态位/箭头跟文字
- 工具行没有主语就空着
- 排队消息画在对话末尾
- 撤回图标 + 词内下划线不斜体 + 开合逐帧贴合
- 发出消息后回到底部
- Artifact 面板通电 + 内置 ECharts + svg/csv/源码高亮
- Goal FK 用会话库 / 子代理渲染复用主时间线 / WebUI 子代理走 Full 档
- *(persona)* 开发工作全权委派 dev 子代理 / 补作息与玄学口径
- Prepare Miyu 0.6.0 with verified Linux packages and OOBE gallery
## [0.5.0] - 2026-09-06

### 🚀 Features

- *(tools)* 工具加载模式降到模型粒度,hybrid 档退役
- *(persona)* 内置脚本与技能默认绑定 Miyu 出厂人格
- *(tools)* 默认工具加载模式改 full
- *(scripts)* Flight-deals 机票比价进内置索引,schema 瘦身
- *(llm)* Antigravity(agy CLI)内置特殊供应商
- *(scripts)* Bangumi 番组日历脚本(放送表/搜索/条目详情/剧集)
- *(webui)* 附件落盘并放开类型、分享文件页面上传、任务面板不再压正文
- *(vision)* 多模态主模型直接看图——媒体块进工具结果,落库逐字节重放
- *(webui)* 插件 dashboard 基建 + 记忆浏览器面板(demo)
- *(webui)* Dashboard 基建二期 + 记忆面板扩完整
- *(webui)* 知识库面板
- *(webui)* 表情包面板
- *(webui)* 群聊与群管面板;发言榜查询去掉无索引窗口连接
- *(webui)* 好感·情绪面板(好感度标签)
- *(real_context)* 情绪状态 + 好感·情绪面板情绪标签
- *(webui)* Dashboard 动效层;「群聊」改名「QQ 消息记录」
- *(webui)* 好感度榜单表头可点击排序
- *(webui)* 设置页重做——三层结构、零手写 JSON、QQ 平台页、拉取模型接口
- *(webui)* 面板批量选择 + 表情包「添加理由」+ 亮色主题侧栏修正
- *(qq)* 视频与文件统一走懒下载链路，vision_analyze 可按 file id 看视频
- *(vision)* Agy 线上 vision_analyze 改为下载后交出路径，由模型 view_file 自看
- *(voice)* 语音 v2——唤醒对话、本地识别、三入口听写、独立 miyu-voice 进程
- *(voice)* MiniMax 播报、语音/TTS 双开关、QQ 语音消息与终端发 QQ、音色浏览
- *(embedding)* 内置本地语义检索——bge-small-zh int8 资产 + ONNX Runtime worker，记忆/表情包/知识库/被淘汰上下文关键词+语义融合
- *(scripts)* 脚本头部为真相源、manage_script 自动复制注册与 list、argv flags、目录指纹重扫
- *(webui)* 控制台新增「脚本」面板——脚本工具清单/抽屉详情/禁用·启用·删除/补描述注册
- *(webui)* 脚本面板按人格切视角
- *(config)* 分级模型池——四档 lite/cheap/standard/flagship、旁路请求分档、通讯平台池引用
- *(embedding)* Worker 子进程以 nice 10 运行，建索引时让路给前台
- *(voice)* 语音与 QQ 八项打磨 + 语音全灭事故修复(夹具与文档)
- *(voice)* 小米 MiMo 播报供应商 + 追问窗口 30s 从播完起算 + miyu listen 开关 + 音色列表即时搜索
- *(voice)* 通知标题改 Miyu、关闭提示音 off、MiMo 语速档位

### 🐛 Bug Fixes

- *(qq)* 批量撤回的 id 不再被引用目标顶掉,他人消息也能批量撤
- *(memes)* 移除 avoid 字段——它让整个表情库对"用户求助"自我禁用
- *(qq)* 覆盖只接管已承诺的回复,并保留原始触发标签
- *(claude-code)* 工具面按档位分叉 claude 会话——清单增删不再被读成"工具掉线"
- *(repl)* /persona 切换后重绑会话,不再卡死在加载中
- *(scripts)* Crack-search 跟进 crackrelease 改版，并把解析失败与无结果分开
- *(tools)* 加载模式只看主回合池,多模态池不再拖全为 full
- *(daemon)* 事件流 resync 不再把并发会话的正常回合误报「已取消」
- *(relay)* 评审修复——并发回合下的全局落盘物、错误分类、脚本边角
- *(config_tui)* 内置 CLI 供应商的模型目录问 CLI 要,不再等于"已激活"集合
- *(qq)* 撤回/取代导致的回合取消静默收场,不再回"出错了:本轮被取消了"
- *(relay)* CLI 中转线多模态入口 + followup 尾巴化石化 + 图片哈希投影
- *(mcp)* 不可达 MCP server 不再卡死 daemon 启动 (#36)
- *(webui)* 手机端布局——控制台侧栏改顶部标签条、分享面板工具条换行、设置导航行不再拉伸
- *(config)* 拆开「模型具备的模态」与「能塞进消息的模态」，agy 模型可进多模态池
- *(relay)* 续传映射落盘 + CLI 输入字节预算,agy 全量重放不再砍掉本轮消息
- *(repl)* Footer 声波律动三档色全部走 matugen 绑定的终端语义色
- *(webui)* Dash-chip 禁止内部折行，窄列里「index 覆盖」不再挤成两行
- *(qq)* 语音消息独占一条,引用/@ 挂到第一条非语音帧
- *(voice)* Miyu listen 再按一次真停 + 通知中文 + MiMo 风格标签改多选
- *(config-tui)* 无按钮表单补 string_list/dialog_list 回车臂(唤醒词回车进列表);MiMo 语速并入提示词
- *(qq)* 发过语音不再发正文;零宽空格等不可见字符当空回复;MiMo 提示词标签缩短

### 📚 Documentation

- *(plan)* Antigravity(agy)供应商调研与方案评审;09-03 五维优化调研
- 清理已执行完毕的案卷与方案文档
- 更新 todolist

### 🚜 Refactor

- *(persona)* 内置脚本/技能源码按 personas/ 布局,人格门改隐式
- *(llm)* 抽出 cli_relay 共用骨架;新增 Codex(codex CLI)内置特殊供应商

### 🎨 Styling

- Cargo fmt 收干净(12 文件,纯换行/缩进)
- Cargo fmt 全仓库

### 🧪 Testing

- *(codex)* 真机续传探针(PTY 两轮,第二轮 resume <id> 只发增量)
- 工具 schema token 预算测试;桥问答黏标签的 PTY 回归探针进 testkit
- Schema 预算只管内置工具(脚本按「一个脚本包办」设计,不约束)

### ⚙️ Miscellaneous Tasks

- *(claude-code)* 清理配置注释漂移与委托工具残留
- *(tests)* 清掉测试构建的 9 条警告
- Update Cargo.lock for v0.4.7

### 💼 Other

- *(miyu)* 人格设定精简与合并
- *(miyu)* 搜索要求限定在知识问答;精简预设对白;新增 travel-planner 技能
- *(arch)* 运行库依赖改为虚包 onnxruntime，cpu/cuda 两个包都能满足
- *(config)* 模型池相关菜单只显示本地化名字，Embedding 菜单收本地模型进单选
- V0.4.7
- V0.5.0
## [0.4.6] - 2026-08-31

### 🚀 Features

- Vision_analyze 支持视频输入
- QQ 工具面矫正与注入文风英文化
- *(webui)* 生图点阵气泡与工具卡改造
- 占卜四法分显示名(divine:method 事件名)
- *(tui)* QQ 定时消息表单三改
- 工具期风格锁——persona-ab 工具体制 A/B 实测胜出后定稿
- 图片链路四改——占位符瘦身/视觉批量/shell-hook 短链名/Path 图直读
- 观测与桌面体验杂修六件
- *(qq)* 会话内并行开关、工具桥打通平台会话、消息判读三改、用量细项
- *(webui)* 统计页细项进饼图、按来源筛选,侧栏会话名去粗体
- *(webui)* 数据统计页加"清空"按钮
- *(qq)* 合并转发递归展开,历史库只存摘要
- *(vision)* 视频当附件内联,规格对齐 GLM
- *(bili)* 直播脚本加改标题/改分区,更名 bilibili_live_stream
- *(qq)* 上一条是自己发的就引用,连发时不再失去指向
- *(qq)* 私聊也能引用历史图片
- *(qq)* 概率抽中且判官放行的回合,注入一句"照样接话"
- *(scripts)* 脚本缓存统一到 MIYU_SCRIPT_CACHE_DIR,默认落在 .miyu/cache
- *(scripts)* 联网脚本登录改成两步式,AI 可以全程引导
- *(scripts)* 四个脚本转为内置，小红书反检测重做

### 🐛 Bug Fixes

- 生图静默失败三连修与 conversation.db 损坏根因加固
- 占卜显示名改用用户命名(六十四卦/塔罗牌/吉凶占)
- QQ 复读双修——文字投递幂等闸与端点重试半截正文丢弃
- QQ 回复定向 system 常量——历史块只是背景,不是待答清单
- 防失忆提醒三改——文案纠偏/默认值实测定 3/工具后重锚证伪记录
- 工具复读全链治理——回放折叠根治毒料自增殖+执行闸二版+泄漏过滤
- Web_fetch 乱码——reqwest 启用 gzip/deflate 透明解压
- *(llm)* 端点冷却连败指数退避——持续故障不再每两分钟被重新信任
- *(qq)* 消息判读全链强化——坐标署名/引用三重标记/并发防线三层
- *(qq)* 群管踢人核验二版与撤回三改
- *(webui)* 当前会话名不再整行加粗
- *(qq)* 主动回复判断并行,回复仍按到达顺序串行
- *(bridge)* MCP 桥补上作用域看图与生图
- *(shell-hook)* 显式路径要真能执行才算命令
- *(repl)* 等待计时器每次工具跑完重新起算
- *(repl)* 粘贴三修——视频进附件通路、Video 标签、折叠判据
- *(model)* 会话模型覆盖失效时自动回退,别把入口锁死
- *(qq)* 跳过主动判断的三条路径留痕,限额路径补 trigger
- *(bridge)* Artifact 与分享工具收口到 WebUI 回合
- *(qq)* 唤醒关键词不再从正文里剥掉
- *(state)* 坏库不再吞掉备份,启动失败带上原因
- *(qq)* 回合上下文不再声明有没有被 @
- *(qq)* 私聊回合被新消息取代,不再先吐半成品
- *(qq)* 私聊会话 id 取对方而不是自己
- *(qq)* 自己踢人时移除台账也要记,核验才可能生效

### 📚 Documentation

- 工具 wiki 重写与 token-diet 专项全档;用户待办与人格微调
- AGENTS.md 项目注意事项 + 已完成计划归档清理
- AGENTS.md 补 rustfmt 递归刷子模块的坑
- AGENTS.md 5.5 改口径——仓库已 fmt-clean,直接 cargo fmt

### 🚜 Refactor

- 工具面统一(edit/read/kb/artifact 三域)与 token 瘦身

### 🎨 Styling

- Cargo fmt 全量格式化,仓库转为 fmt-clean

### 🧪 Testing

- 稳住 PTY 提示符探测与 kitty 终端下的数学渲染测试
- *(repl)* 终端光标记账的量尺
- *(qq)* 假 OneBot 客户端测具,真实驱动群聊回合
- *(qq)* 假 OneBot 测具加私聊模式与真图素材

### ⚙️ Miscellaneous Tasks

- *(packaging)* PKGBUILD bump 0.4.5
- *(packaging)* 三份 PKGBUILD 与 0.4.4 起的实际发版结构对齐
- *(persona)* 人格与防失忆提示文案更新(用户改写)
- *(qq)* 出站引用段留痕
- Scripts 改名为 test_scripts,脚本路径改为跟随自身
- *(assets)* 加入 B 站直播推流码获取脚本
- Todolist 更新
- *(repl)* REPL 会话指针解析失败时打出原因
- *(persona)* 加入高危命令禁令,移除 shorin 外貌描写(用户改写)
- *(assets)* 加入知乎/小红书检索与酒店比价脚本
- Update Cargo.lock for v0.4.6

### 💼 Other

- Token 瘦身与工具面统一专项(perf/token-diet)
- V0.4.6
## [0.4.5] - 2026-08-21

### 🐛 Bug Fixes

- Responses 续传不支持的自愈签名放宽(#32) + haiku 去思考档

### ⚡ Performance

- 低占用专项——三轮迭代降内存/CPU/合成负载

### 💼 Other

- V0.4.5
## [0.4.4] - 2026-08-20

### 🚀 Features

- *(memory)* Reset-memory 三条路都去掉二次确认
- *(models)* `miyu models -g` 直接编辑全局激活模型池
- *(image)* 生图支持参考图（图生图）
- *(memory)* 通讯平台的 reset-memory 也去掉二次确认
- *(models)* 菜单里加「继承全局模型池」；deepseek 状态只回结论
- *(render)* Todowrite 和批量工具调用也给准备提示
- *(config-tui)* 多模态与语义模型列表补上 [d] 删除
- *(config-tui)* [u] 分步撤销删除
- *(repl)* Ctrl+←/→ 按词跳光标；Ctrl+W 隔着空白也整块删占位符
- *(web)* WebUI 支持斜杠命令，命令表上提到 crate 级与 CLI 同源
- *(web)* 命令平面补 /reset、工具签落盘可见、图片改灯箱预览
- *(web)* 正文旁常驻任务面板
- *(goal)* 同会话长任务目标——三件套工具、/goal 命令与续轮驱动器
- *(game_compat)* 接回 AreWeAntiCheatYet，三源合一返回 Markdown
- *(web)* 命令平面与时间线按实测清单修——回执不再乱跑
- 2026-08-18 计划五项施工落地——英文化/文件分享/定时消息/天气/生图限流
- 接入 Claude Code 订阅三件套——claude-code 供应商中转/MCP 工具桥/claude_code 委托工具
- Claude-code 工具面双四档——原生工具转正(默认 all)/Miyu 工具桥改作用域(默认 off)
- *(web)* 前端批——音频播放卡/圆角8/分享面板多选/删末会话闩锁/share 富预览重建
- Claude-code 第十二轮——平台门禁撤销(QQ 可用) + 侧栏会话拖拽排序
- 平台生图改显式发送(裁定) + REPL footer 运行转轮
- *(renderer)* 长文转图代码等宽字体(JetBrains Mono)+圆角背景
- *(repl)* 声波律动运行动画(替换盲文转轮) + 回车光标瞬移修复
- 平台最大工具轮数可配置(默认32) + 重复调用提醒改第5次起持续

### 🐛 Bug Fixes

- *(vision)* 当前文本模型自带眼睛时 vision_analyze 就用它
- *(replay)* 半截 JSON 的工具参数不再写进历史，也不再被回放
- *(image)* 生图参考图的作用域不再由看图插件把门
- *(tools)* Review 收尾——合并遗留的悬空引用与死参数
- *(image)* 参考图收下模型真会传的几种形状，并且不再静默失效
- *(tools)* 数组参数统一收下模型真会传的几种形状
- *(tools)* 模型传来的畸形参数改在收口处统一处理
- *(tools)* 数字/布尔被模型写成字符串时按 schema 还原
- *(moegirl)* 长条目和 501 都不该报「页面不存在」
- *(qq)* 命令输出也走回复处理插件
- *(repl)* 回合中途的 token 计量补上 IPC 这一段
- *(render)* 批量准备提示的计时不再每个工具归零
- *(terminal)* 图片印成满屏——daemon 模式下尺寸在 CLI 侧丢了
- *(math)* 非 kitty 的公式行数随内容走，不再一律撑到 9 行
- *(terminal)* 图片尺寸、公式行数与渲染层记账
- *(math)* 非 kitty 终端的公式改走 chafa，不再是像素块
- *(math)* 字体预热必须用带 CJK 的公式
- *(repl)* / 开头不命中命令表的输入当成普通消息，回车不再做前缀展开
- *(tools)* Ask_question 改 always_loaded，交互面首轮就发完整契约
- *(repl)* 普通 REPL 自举独立会话，不再落进终端集成那条车道
- *(web)* 冷启动的上下文快照现算，footer 不再显示 0
- *(repl)* 输入历史按会话分文件，上键翻之前现读一次
- *(jobs)* 后台任务的可见性收紧到本会话，直连道也过滤
- *(onebot)* 入站文件返回路径前必须 flush——不 flush 会把空文件交给模型
- *(footer)* 上下文窗口是猜的就别再算百分比
- *(agent)* 每个工具轮落一次 tool_flow，崩溃后模型知道自己已经做过什么
- *(skills)* Load_skill 改常驻，否则模型看不到任何技能名
- *(todo)* 重置对话时连待办一起清
- *(web)* 修会话切换与刷新的三处走丢，侧栏改用盲文点阵
- 迁移器不再替前人背锅；二进制被换掉后不再静默瘫痪
- *(goal)* 续轮提示词自报来历——不然模型把它当注入拒掉
- *(goal)* 空转一轮就停手；续轮提示词照 dsh 精简并去掉硬编码的名字
- *(goal)* 按四轮实测验收重做交互语义——edit 即中断重开，回执有据可查
- *(llm)* Usage 帧也算「说完了」——muse-spark 不发 [DONE] 也不给 finish_reason
- *(web)* 巨型 turn future 装箱落堆——with_image_gen_limit 加层后撞穿 16MB 栈
- Claude-code 第四轮验收六问题——清空联动/图片桥路由/WebUI 工具过桥/去重/置顶/200k 窗口
- Claude-code 第五轮——幽灵 daemon 破案/wake 流补图片/工具摘要/autocompact 同步/去重扩表
- Claude-code 第六轮——MCP 桥连错 daemon 的总根因/工具卡片化/autocompact 跟随窗口/daemon stop 加固
- Claude-code 第七轮——artifact 过桥/上下文表真值/ask_question 过桥/后台跟进/时区
- Claude-code 第八轮——task 子代理经中转的空工具集真相与修复/task 请回去重表
- Claude-code 第九轮——切走丢卡片正式修复(remote 车道)/环境事实注入/后台闭环/REPL 原生工具展示
- Claude-code 第十轮——Bash 展示与 run_command 同构 + 图片上方只空一行
- *(web)* 回合进行中切走再切回不丢已渲染输出——直播状态离屏保活
- *(qq)* 引用归属/群管权限语义/定时消息表单/生图投递观测与自救链
- *(qq)* 发图幂等闸(同图4连发) + /stop 回复限时
- *(repl)* Footer 孤儿行(双footer/取消残留) + 波浪三空格 + 第二处同步块内查询
- 同参工具调用熔断 + 平台回合轮数兜底 + REPL 取消路径熄波浪
- Token 消耗查询工具不再返回金额估算

### 📚 Documentation

- *(wiki)* 添加 Miyu 模块化中文介绍文档（DeepSeek 临时版本）
- 删掉已完成的拆分方案，补性能优化的实测记录
- *(perf)* M4-6 / M4-7 量完，判定不做
- *(perf)* M3 快照体积量完，判定不做
- *(perf)* 补 G1/G3 显存两条与该批收口
- *(perf)* 补 C27 启动超时与 M6-2 下载上限
- *(perf)* 补 M1/C5/C8/C13、VACUUM 一条与六条不做的判定
- *(perf)* 补 M2 子表迁移与三条连带路径
- 删掉已完成的性能方案目录，G2 原文留存到 fixed
- *(perf)* 开头那句指向已删目录，改成指第二十五节
- *(tools)* Task 的工具描述讲清子代理是全新上下文

### ⚡ Performance

- *(tokens)* 全面削减请求体积（新会话 -36%，长对话 -17~24%）
- *(cpu)* Strip_inline_markup 从 O(n²) 降到 O(n)
- *(cpu)* 知识库关键词检索挪出异步线程
- *(cpu)* 被丢弃的入站事件不再深拷贝整份配置
- *(mem)* 登录限流表不再无界增长
- *(mem)* 封禁理由历史与天气缓存补上界
- *(mem)* Agent 丢掉时停掉 keepalive 循环
- *(mem)* 任务日志改 seek 读，不再整读
- *(web)* WebUI 改用显示尺寸的图，GPU 纹理 30 → 3.7 MiB
- *(terminal)* Kitty 传图只缩不放，小图省 95-98% 传输量
- *(startup)* 默认知识库更新检查加 5 秒上限，最坏从 135 秒降到 5 秒
- *(tools)* 两条图片下载改成边读边卡上限，别整读完再判
- *(web)* 用量统计的两个 handler 挪出异步线程
- *(state)* Conversation.db 补上 auto_vacuum，本机还回 68 MB
- *(tools)* Caniplayonlinux 目录扫描加并发与时间预算，34 秒 → 8.4 秒
- *(web)* 360/搜狗的跳转解析改成保序并发，最坏 120 秒 → 30 秒
- *(agent)* 工具目录的 token 估算加记忆表，每轮省约 110 ms
- *(state)* 工具报告改成追加型子表（schema v25），写入 40.6× → 1×

### 🚜 Refactor

- 按实测基准拆分全部模块，并修若干工具问题
- *(config-tui)* 供应商菜单不再暴露「当前模型」与激活星号
- *(web)* 拆掉顶栏，设置进控制台，模型与思考档位并成一个面板
- Claude Code 从协议改判为内置特殊供应商——默认禁用/专用表单/预置模型/思考档
- *(config_tui)* Claude Code 专用表单拆到独立文件

### 🎨 Styling

- Daemon_cmds 格式回归修正

### 🧪 Testing

- *(kitty)* 图片占位诊断探针
- *(terminal)* 图片网格与活动区重绘的诊断开关
- *(terminal)* 轨迹补上「存的行号 vs 终端实际报的」对比
- *(terminal)* 量尺里的公式那行改成真实形态
- *(gate)* 撤掉 PTY 用例的门禁豁免——那条豁免的归因是错的
- *(agent)* 关掉测试里的自动表情包骰子——那条「并发 flake」根本不是并发

### ⚙️ Miscellaneous Tasks

- *(packaging)* 0.4.3 资产版本与校验和
- 用例数基线 1518 → 1536
- AGENTS.md 换成个人工作指令；todolist 清掉已完成条目
- 收录发版前的工作树改动(人格提示词/todolist,用户手改)
- Update Cargo.lock for v0.4.4

### 💼 Other

- Token-optimization —— 省 token 与终端渲染修复
- Batch-a —— REPL 会话与输入、工具契约、两个丢数据的 bug
- Turn-durability-and-goal —— turn 工具流检查点与 WebUI 命令平面
- Game-compat-awacy —— AWACY 三源合一与 goal 四轮验收整改
- Muse-spark-usage-tail —— usage 尾帧收尾修复 + 2026-08-18 计划五项施工
- Claude-code —— Claude Code 订阅接入全家桶与 08-20 十余轮真机整改
- V0.4.4
## [0.4.3] - 2026-08-16

### 🚀 Features

- *(qq)* AI-based group join approval plugin
- *(platforms)* Group file history and platform file reader

### 🐛 Bug Fixes

- *(review)* 按 08-16 五轮 review 逐项验证修复约 65 处缺陷

### ⚙️ Miscellaneous Tasks

- *(packaging)* 0.4.2-3 补丁资产版本与校验和

### 💼 Other

- V0.4.3
## [0.4.2] - 2026-08-16

### 🚀 Features

- *(jobs)* Shellhook 后台任务完成后把跟进回复写回触发它的终端
- *(jobs)* 终端回写改流式——思考/工具/正文随唤醒回合逐行追加
- *(context)* 照 dsh 结构化历史回放 + 工具输出 spill 外溢
- *(persona)* 防失忆提醒+预设对话全链落地,附逐请求计量与表情包平台开关
- *(usage)* Models.dev 计费估算+手动定价;统计页图表修复与移动端适配
- *(memes)* 终端/WebUI 的自动提示发送表情默认启用
- *(state)* 「默认会话」改名「终端集成会话」(v21 迁移)
- *(tools)* Registry 级兜底超时,防无自管超时的工具挂死回合
- *(tools)* 单调 Guard 层落地,AUR 互斥迁入,新增命令拒绝子串
- *(agent)* Repeat-tool-reminder 防死循环提醒(dsh 同款,advisory-only)
- *(llm)* Responses 续传乐观自愈,修 opencodego/DeepSeek 400 (任务#5+#16)
- *(mode)* [**breaking**] 删除闲聊模式,新增 Build/Dev 极简开发模式 (任务#6+#7)
- *(mode)* Miyu normal/dev 双入口,dev 会话挂保留人格、模式按记录定死 (任务#8)
- *(state)* Goals 表(迁移 v22)+ CAS 目标状态机 (任务#9)
- *(goal)* 三件套工具 + /goal 命令 + 自主轮 wrapup (任务#11)
- *(goal)* 同会话续轮驱动器——Miyu 能自己把长任务干到底了 (任务#10)
- *(bridge)* Miyu tool-call 工具桥——bash 即编排层 (任务#12)
- *(tui)* 自定义提示词三分结构 + 预设对话列表式编辑器
- *(session)* 归档整体移除+列表类型标+WebUI 模式分组;REPL footer 同源修复
- *(dev-tools)* 极简裁剪定稿——凡 coreutils 干得更好的都不注册
- *(edit)* Apply_patch 定为唯一编辑器——normal 同步裁剪,Artifact 补删除,渲染三态化
- *(memory)* /reset-memory——按模式清空长期记忆,会话历史保留
- *(cli)* Reset-memory 快捷命令;提示语去掉「会话历史」尾巴
- *(daemon)* 前端退出不拖死会话任务(dsh 语义);WebUI 工具家族图标
- *(debug)* 出网请求录制与实时监控——miyu daemon logs request
- *(cache)* 尾巴瘦身与化石违约修复——逼近 dsh 的每轮零新增形态
- *(cache)* [**breaking**] 跨轮思考回放退役;平台 /reset-memory;图标微调
- *(cache)* Dev 请求瘦身 22%;联想自回声过滤——稳态尾巴收敛到纯对话本体
- *(webui)* 控制台缓存命中率显示两位小数,>99.99 封顶显示 100%
- *(dev)* 补上分析图片(vision_analyze)

### 🐛 Bug Fixes

- *(qq)* 长文转图复活——字体兜底与 debug worker 限额;终端也能查用量
- *(llm)* 429 不再自我加码——限流只花一次请求,窗口按 input 上限算
- *(jobs)* 终端回写垫两行空白,SIGWINCH 重绘不再吃掉正文尾部
- *(jobs)* 回写思考改用 REPL 同款绿色,撤掉分隔线与反编造 hint
- *(cache)* 并发回合完成序追加,消除插入型缓存断点
- *(qq)* 后台任务唤醒回合继承发起者身份,报告直附结果块 (#29)
- *(agent)* 模式权限拦截软失败,bail!→tool error 让轮次存活
- *(repl)* 直连道命令泄漏守门——表内命令不再原文发给模型 (任务#14')
- *(todo)* 按会话落盘,修跨会话共享串味;goal 轮不重置重复链 (任务#13)
- *(build)* 删除幽灵 rerun-if-changed,恢复 release 增量编译
- *(dev)* 会话解析尊重显式 id+独立记忆落地;REPL 杂项验收修复
- *(config)* 模型温度按模型作用域;全局参数补 default_mode;删 $EDITOR 提示
- *(web)* 回合 422 回归修复;终端集成会话锁定;load_tools 未知名一票否决
- *(webui)* 工具标签间距收紧;科学计算/汇率归并计算器图标
- *(webui)* 思考块头部间距与工具标签对齐(gap 5px/padding 9px)
- *(jobs)* 完成的后台任务从注册表移除,不再终身堆积
- PR#31 反馈落地——独立 config TUI 挂断空转与四处无界增长兜底

### 🚜 Refactor

- *(cli)* 验收三轮——CLI 会话命令裁撤+帮助分组重写;dev 视觉与选择器修复

### 🧪 Testing

- *(persona-ab)* 人格提示 A/B 测具(PTY 驱动直连 REPL + DB 轮询)
- *(persona-ab)* 测具适配新入口(miyu normal)

### ⚙️ Miscellaneous Tasks

- *(packaging)* 同步 AUR 包装包 0.4.1
- *(packaging)* 0.4.2 资产版本与校验和
- *(packaging)* 0.4.2-2 补丁资产版本与校验和

### 💼 Other

- 删 codegen-units=1,冷 release 构建 6m13s→2m40s (2.3×)
- V0.4.2
## [0.4.1] - 2026-08-13

### 🚀 Features

- *(tools)* 移入回收站改为批量,一次调用删完整批
- *(jobs)* 后台任务列表带日志尾部,完成时直接返回结果
- *(render)* 终端 LaTeX 渲染——kitty 高清图与半块兜底,表格内上下分式
- *(web)* 控制台数据统计、KaTeX 公式与视频播放,QQ 用量查询工具

### 🐛 Bug Fixes

- *(paths)* 目录迁移把缓存当可丢弃数据，软链接不再卡死启动
- *(cli)* 终端断开后 REPL 不再化作 98% CPU 残留进程
- *(qq)* 群聊当前消息挪到记录块之后,hint 只陈述内容不再下指令
- *(cli)* 输入抽干防积压,挂断看门狗兜住 crossterm 一切自旋形态

### 🚜 Refactor

- 移除计划模式

### ⚙️ Miscellaneous Tasks

- *(packaging)* 终端模式修复发布，pkgrel 提到 2
- *(qq)* 群聊上下文窗口默认 50->25,主动回复判断窗口 30->20
- *(release)* 0.4.1
## [0.4.0] - 2026-08-12

### 🚀 Features

- Expand WebUI markdown rendering
- 多会话支持与 daemon 并行 turn 架构（单二进制）
- *(webui)* MD3 配色改版与多会话侧栏
- 会话定向 turn 与每会话只读快照 API
- *(webui)* 每视图独立会话与多路并行 live
- 子代理并行执行与审计会话
- *(render)* 并行子代理独立状态行
- *(render)* 并行子代理独立块渲染——各自 spinner、原地冻结、整批一次提交
- *(webui)* Matugen/窗边配色方案切换与 live turn 渲染复用
- Phase 4 模型档位——子代理 tier 路由与辅助任务走 cheap
- Task 工具描述动态附加档位池状态（含具体模型名）
- *(cli)* Miyu web 改用子命令风格并新增 restart
- *(web)* 默认端口改为 8300
- *(webui)* 窗边语言细节打磨——签、气泡、停止键与队列布局
- *(platforms)* 接入通讯平台——OneBot v11（NapCat/QQ）桥接
- *(platforms)* 完善 QQ 接入与统一 daemon
- *(platforms)* 新增通讯平台命令与会话重置
- 完成 session、WebUI 重构与腾讯 QQ 接入
- *(platforms)* 支持 NapCat 好友申请白名单处理
- *(platforms)* 完善 QQ 非白名单模型池与配置菜单
- *(cli)* 支持运行时配置热重载
- *(platforms)* 支持主动回复判断自定义提示词
- *(platforms)* 优化自动艾特触发条件
- *(platforms)* 支持跳过指定 QQ 的主动回复判断
- *(platforms)* 支持 QQ 多目标艾特
- *(platforms)* 独立 QQ 文字消息历史
- *(cache)* 前缀缓存命中率提升——append-only 化石化、runtime 移位、保温 ping (v7 实测批次)
- *(compact)* 尾巴保留、防连环四闸、fork 缓存复用、机械 prune 层与被动溢出恢复
- *(compact)* 第二批——tool footprint、QQ 群聊摘要化、TTL 冷恢复剪枝、摘要输出硬帽、byte-prefix e2e
- *(qq)* 群头像与群友头像的查询、查看与下载发送
- *(web)* --bind 参数控制 WebUI 监听地址
- *(ui)* Read_file 摘要显示分页范围；单模型时思考档位弹层直出档位
- *(tools)* Run_command 后台执行——job_status/job_stop 与完成自动唤起
- 后台任务子系统与领域工具 skill 化
- Support Shift+Enter newline in REPL
- 生图工具开放给受限平台（QQ）；新增 goofish 闲鱼搜索自带脚本
- *(jobs)* 后台任务会话隔离（job_status/job_stop/状态条，all=true 跨会话）；SIGHUP/SIGTERM 退出清扫；缓存显示改百分比；子代理块明细缩进对齐
- 会话三车道、准备提示、桌面通知、批量群管与一批 REPL 修复
- *(transfer)* 数据单元注册表与完整性守卫（export/import 基础）
- *(real_context)* 续聊窗口默认 15s、去掉次数上限、回复即续期
- *(transfer)* Miyu export / import 配置与数据移植
- *(tools)* 拦下 AI 幻觉造成的不可恢复删除
- *(config)* 全局设置里加上删除确认开关
- *(usage)* 缓存命中率按输入计算，Σ 纳入子代理，REPL footer 去掉本轮
- *(memory)* 驱逐上下文加 FTS5 索引与时间范围，去掉 1000 条上限
- *(qq)* 群聊改为累积式历史，日志只追加新消息
- *(memory)* 驱逐上下文加语义检索，关键词弱时才补充
- *(memory)* 关联记忆注入带 [YYYY-MM-DD] 日期前缀
- *(memes)* 表情包记录来源者与收发时间（origin 元数据）
- *(web)* 接入 Exa 搜索，免 key 公共额度与冷却回退
- *(tools)* 移除后台子代理与后台命令的运行时长上限
- *(tools)* 子代理默认不限步数,max_steps 转为可选预算
- *(tools)* Job_status 移除阻塞等待——后台任务真正后台化
- *(prompt)* 系统提示词注入 <host-environment>，携带 OS/内核/数据目录

### 🐛 Bug Fixes

- *(render)* 并行子代理逐块堆叠显示
- *(render)* 已提交摘要路径复用堆叠分块构建器
- *(render)* 并行子代理进度按 event name 归属，完成块立即左对齐
- 流式 tool_call 累积器上限、daemon 僵尸收尸、命令输出封顶
- Task 工具描述覆盖层补上 tier 参数（AI 此前看不到档位选项）
- *(repl)* 编辑器回屏时无条件恢复光标可见
- *(llm)* 对上游 5xx 无限退避重试
- *(llm)* 将 5xx 重试退避上限提高到两分钟
- *(platforms)* 移除 QQ 禁言时长上限
- *(shell)* 修复 fish 多行命令拦截
- *(platforms)* 优化长图代码块渲染
- *(test)* README token 向量过期值更新 (2885→2817)
- *(repl)* 底栏上下文水位改回当前上下文估算，不再用请求消耗
- *(queue)* ESC 取消同时撤回排队中的追加消息
- Honor skills.allow_command_execution in run_command registration
- *(kb)* 知识库读取宽容路径解析（前缀省略/相近建议）；人设与 token 向量同步
- 通讯平台里搜图和生图后的图片会被重复发送
- *(i18n)* 工具显示名「网页搜索」改为「网络搜索」
- *(cli)* 问答面板显示不了时不再中止整轮
- *(cli)* 无法显示问答面板时结束提问而不是结束整轮
- *(qq)* 群聊历史重新记录图片,不引用也能追问「这张图是什么」
- *(config)* 生图输出目录认得旧 XDG 根,自愈并搬走旧文件
- *(llm)* 流被提前掐断不再当成成功回复
- *(image)* 生图不再把本地路径写进回复
- *(llm)* 会话中途的上下文块改用 user 角色，跨轮前缀缓存从 29% 回到 97%
- *(repl)* 高延迟 SSH 不再踢出 REPL；后台子代理完成后自动刷新累计
- *(qq)* 群聊图片 ID 改为从消息派生，不再随新图整体平移
- *(question)* REPL 提问正文与选项标题按宽度软换行
- *(daemon)* 核心与整理器线程栈加大到 16MB，防 debug 构建栈溢出
- *(renderer)* 升级 cosmic-text 0.19 修复文字叠加;行内代码加底色块
- *(qq)* 补救覆盖窗口重做——承诺后免判断顶替,表情随覆盖转移
- *(tools)* Task 描述文件同步「步数默认不限」
- *(qq)* 带附件的发送不再按体积限时,四条投递告警补 miyu::qq
- *(repl)* 「准备编辑/执行/任务」提示从来没显示过
- *(repl)* 问答面板不再开 kitty 键盘协议——输入框变成 [17u 的元凶
- *(qq)* 踢人接口的假失败按成员实际状态判定
- *(repl)* 问答面板不再掀掉 REPL 的终端模式——回答后输入框失灵

### 📚 Documentation

- 多会话与 daemon 并行架构设计文档
- 上下文与缓存设计理念（文章体）
- Todolist 清理——移除已修复的两个 BUG 与已完成的上下文触限压缩
- Todolist 移除已完成的 read_file 分页日志与 webui effort 两项
- Todolist 移除已完成/已证实无问题的两项
- Todolist 更新（用户新增 REPL 内存与会话管理两项观察）
- V0.4.0 发布说明配图

### ⚡ Performance

- *(qq)* Runtime 戳在平台回合只留时间，去掉终端专属字段
- *(qq)* 主动回复判断把规则前置，命中率 0 → 93~98%
- 上下文与热路径整体优化批次

### 🚜 Refactor

- 移除 miyu web 自动打开浏览器及 --no-open 选项
- 子代理档位改为模型池，AI 自选档位，辅助任务与档位解耦
- *(qq)* 移除群聊历史摘要
- *(reset)* 三个前端的 reset 语义统一；reset all 独立为 wipe
- *(qq)* 上下文条数拆成回复窗口与判断窗口
- *(config)* 语义模型提升为顶层配置，并可在模型上标记
- *(qq)* 群管查询三合一，支持排序与管理员跨群查询
- *(prompt)* 说话方式一节收紧为段落式并强调句式模仿
- *(memory)* 联想块开头明说「不要模仿记忆里的对话」

### 🎨 Styling

- 统一 Rust 代码格式

### 🧪 Testing

- 修掉三条把易变环境当固定夹具的测试
- 修两条不稳定的测试

### ⚙️ Miscellaneous Tasks

- *(tools)* 始终加载知识库和记忆工具
- *(packaging)* 填入 v0.4.0 资产校验和并刷新 miyu-git 版本快照
- *(packaging)* V0.4.0 资产替换后更新校验和与版本快照
- *(packaging)* 无时限版资产替换后更新校验和与版本快照
- *(packaging)* 最终资产替换后更新校验和与版本快照
- *(packaging)* Task 描述修正后更新校验和与版本快照
- *(packaging)* Host-environment 落地后重刷 v0.4.0 资产与校验和
- *(packaging)* 送信超时修复落地后重刷 v0.4.0 资产与校验和
- *(packaging)* 四项修复落地后重刷 v0.4.0 资产与校验和
- *(packaging)* Miyu-git 快照对齐实编译结果 r433.gae3b5e9

### ◀️ Revert

- *(tools)* 移除删除防御(delete_guard)

### 💼 Other

- *(arch)* 新增 packaging/arch/PKGBUILD（miyu-git VCS 包）
- *(webui)* 回复 footer 文案——tokens→词元、缓存 xx→xx 缓存命中
- V0.4.0
## [0.3.0] - 2026-07-25

### 🚀 Features

- Add local web interface

### 🐛 Bug Fixes

- Preserve reasoning-only completions
- Stabilize live repl rendering and queue lifecycle
- Preserve cursor visibility during live output
- Preserve full reasoning color across chunks
- Stabilize inline live repl streaming
- Stabilize kitty images during live scrolling

### ⚙️ Miscellaneous Tasks

- Update Cargo.lock for v0.3.0

### 💼 Other

- V0.3.0
## [0.2.1] - 2026-07-19

### 🚀 Features

- Stream native reasoning summaries
- Add conversation context pop
- Add variant CLI command
- Add fish command completions
- Add bilingual interface localization

### 🐛 Bug Fixes

- Soften patch diff colors

### ⚙️ Miscellaneous Tasks

- Update Cargo.lock for v0.2.1

### 💼 Other

- V0.2.1
## [0.2.0] - 2026-07-18

### 🚀 Features

- Improve activity summary topics

### 🐛 Bug Fixes

- Improve LLM transport diagnostics
- Keep full reasoning output unstructured

### ⚙️ Miscellaneous Tasks

- Update Cargo.lock for v0.2.0

### 💼 Other

- Integrate activity summary topics
- V0.2.0
## [0.1.22] - 2026-07-17

### 🚀 Features

- Add thinking variant selection

### 🐛 Bug Fixes

- 根据Code Review优化extra_body实现
- 修复extra_body配置校验与字段冲突
- 仅在 pop 模式加载旧上下文搜索工具

### ⚙️ Miscellaneous Tasks

- Update Cargo.lock for v0.1.22

### 💼 Other

- V0.1.22
## [0.1.21] - 2026-07-17

### 🐛 Bug Fixes

- 修复工具输出渲染与模型命令

### ⚙️ Miscellaneous Tasks

- Update Cargo.lock for v0.1.21

### 💼 Other

- V0.1.21
## [0.1.20] - 2026-07-16

### 🚀 Features

- 实时显示命令输出
- 优化多源图片搜索
- 增加结构化用户提问交互

### 🎨 Styling

- 调整思考与工具组显示

### ⚙️ Miscellaneous Tasks

- Update Cargo.lock for v0.1.20

### 💼 Other

- V0.1.20
## [0.1.19] - 2026-07-15

### 🐛 Bug Fixes

- 完善上下文压缩与模型窗口处理

### ⚙️ Miscellaneous Tasks

- Update Cargo.lock for v0.1.19

### 💼 Other

- V0.1.19
## [0.1.18] - 2026-07-13

### 🐛 Bug Fixes

- 修复 Miyu 模型端点回退异常

### ⚙️ Miscellaneous Tasks

- Update Cargo.lock for v0.1.18

### 💼 Other

- V0.1.18
## [0.1.17] - 2026-07-13

### 🐛 Bug Fixes

- 修复 diff 渲染软换行

### 🎨 Styling

- 调整运行命令的工具输出样式

### ⚙️ Miscellaneous Tasks

- Update Cargo.lock for v0.1.17

### 💼 Other

- V0.1.17
## [0.1.16] - 2026-07-13

### 🐛 Bug Fixes

- 修正 token usage 获取与显示语义
- 用有效请求上下文估算 token 占用

### ⚙️ Miscellaneous Tasks

- Update Cargo.lock for v0.1.16

### 💼 Other

- V0.1.16
## [0.1.15] - 2026-07-12

### 🚀 Features

- Balance web search providers
- Balance active model endpoints
- 完善文本与多模态模型配置
- 接入 MCP stdio 工具
- 完善补丁编辑与工具状态展示

### 🐛 Bug Fixes

- Refine active model selection
- Hide unconfigured Anthropic model
- Align mixed model UI
- Scroll models selector
- Update mixed context and scrolling
- 简化全局设置返回流程

### ⚙️ Miscellaneous Tasks

- Update Cargo.lock for v0.1.15

### 💼 Other

- V0.1.15
## [0.1.14] - 2026-07-12

### 🚀 Features

- Support Anthropic messages protocol
- Enable Anthropic thinking

### 🐛 Bug Fixes

- Set Anthropic context window
- Use Anthropic context window fallback

### 🎨 Styling

- Cargo fmt fixes

### ⚙️ Miscellaneous Tasks

- Update Cargo.lock for v0.1.14

### 💼 Other

- V0.1.14
## [0.1.13] - 2026-07-11

### 🚀 Features

- Add hybrid tool catalog
- Refine REPL composer UI
- Improve repl compaction controls
- Configure meme search result limit
- Add fish seamless enter hook
- Finish fish seamless shell integration

### 🐛 Bug Fixes

- Simplify token usage display
- Stabilize fish seamless prompt replay
- Expand fish abbreviations before hook classification
- Stop fish ctrl-j repaint flicker
- Keep meme tools loaded in chat
- Smooth fish seamless input replay
- Improve fish multiline command detection

### ⚡ Performance

- Stabilize repl prompt prefix

### ⚙️ Miscellaneous Tasks

- Polish repl command hints
- Refine repl models menu
- Update Cargo.lock for v0.1.13

### 💼 Other

- V0.1.13
## [0.1.12] - 2026-07-10

### ⚙️ Miscellaneous Tasks

- Update Cargo.lock for v0.1.12

### 💼 Other

- V0.1.12
## [0.1.11] - 2026-07-09

### ⚙️ Miscellaneous Tasks

- Update Cargo.lock for v0.1.11

### 💼 Other

- V0.1.11
## [0.1.10] - 2026-07-08

### 🐛 Bug Fixes

- Allow removing active provider models

### ⚙️ Miscellaneous Tasks

- Update Cargo.lock for v0.1.10

### 💼 Other

- V0.1.10
## [0.1.9] - 2026-07-07

### 🚀 Features

- Configure web search result count

### 🐛 Bug Fixes

- Hide pop context messages

### ⚙️ Miscellaneous Tasks

- Reduce meme auto-send probability
- Update Cargo.lock for v0.1.9

### 💼 Other

- V0.1.9
## [0.1.8] - 2026-07-07

### 🚀 Features

- 上下文溢出自动 compact/pop + models.dev 查询
- Optimize interactive paste placeholders
- Add lazy built-in tool loading
- Derive script descriptions from headers
- Add lightweight chat mode
- Enhance weather tool with Open-Meteo

### 🐛 Bug Fixes

- 并发对话 running turn 被误恢复为 interrupted
- Shell-init 类命令不再触发 miyu init；修复 init 时 shell 菜单光标移动重复刷新
- 恢复并发对话 owner_pid 机制和 system-reminder 占位符
- Enforce lazy tool loading gate
- Keep script tools out of load_tools
- Make chat mode read-only

### 🚜 Refactor

- Clean up lazy tool defaults
- Replace yolo mode with normal mode
- Compress mode reminders

### ⚙️ Miscellaneous Tasks

- Checkpoint current workspace changes
- Update Cargo.lock for v0.1.8

### 💼 Other

- Fix 并发对话 running turn 被误恢复为 interrupted
- Fix 并发对话 running turn 被误恢复为 interrupted
- Integrate main branch updates
- Tool system reform
- V0.1.8
## [0.1.7] - 2026-07-05

### 🚀 Features

- Fish 终端无缝对话图片粘贴支持

### ⚙️ Miscellaneous Tasks

- Update Cargo.lock for v0.1.7

### 💼 Other

- Chafa依赖
- V0.1.7
## [0.1.6] - 2026-07-04

### 💼 Other

- V0.1.6
## [0.1.5] - 2026-07-04

### 🚀 Features

- Web_search 爬虫 fallback 移植 Yahoo/360/Sogou 多引擎，修复 DDG URL 解包
- Scripts 接口、Arch 新闻工具、stdin 传入、--stdout 选项、移除 web-search skill 硬编码
- Add atomic todo updates and render todo table
- 新增 query_caniplayonlinux 默认工具，重命名 linux_game_compatibility 为 deep_research_linux_game_compatibility
- REPL 粘贴剪贴板图片 + models.dev 视觉能力检测 + 非视觉模型自动降级
- 表情包挑选时显示「思考 · 挑选表情包」进度提示，消除卡顿感
- Ctrl+V 剪贴板路径识别 + 占位符原子化编辑
- 新增 task 子代理工具，提取 SubagentRunner 统一子代理调度
- 新增 edit_string 和 write_file 文件编辑工具
- Add read clipboard tool
- Parallel session via SQLite turns table with pending/interrupted status

### 🐛 Bug Fixes

- Full reasoning 模式下工具摘要行重复且第二次显示为绿色
- Scientific_calculator 工具添加中文显示名「科学计算」
- Show subagent completion stats
- Preserve spacing before tool summaries
- 修复交互模式下子代理工具 spinner 叠行和样式错误
- 修复工具摘要 finalize 时多余空行导致闪烁
- 交互模式下 LLM 请求报错不再导致退出
- Spinner 从多行变少行时误清除上方空行
- Shell hook 错误信息不显示 + 日日新工具调用 invalid tool_call_id 400
- Spinner 去线程化消除多工具并发渲染竞态；scripts 双目录扫描+stdin JSON 参数+list_scripts 工具
- Refine auto meme context
- Exclude current turn from chat_messages and parallel-focus

### 📚 Documentation

- 更新提示词与 README

### 🚜 Refactor

- Simplify auto meme reminders and remove recent_meme tool

### ⚙️ Miscellaneous Tasks

- 移除旧的 Arch 打包脚本

### 💼 Other

- Parallel session support via SQLite turns table
- V0.1.5
## [0.1.4] - 2026-07-02

### 🚀 Features

- 重构输入法诊断路径矩阵 + spinner 生命周期修复 + deep_diagnose 工具
- Subtool full display, thinking/tool separator, AUR status overhaul, UI tweaks
- 支持 Ctrl+L 清屏
- 新增 protondb_query 插件，查询 ProtonDB 游戏兼容性评级和用户评论
- Subagent reasoning display, dual spinner styles, progress fixes, config & prompt updates

### 🐛 Bug Fixes

- 手动 Ctrl+J 换行不再被折叠为粘贴内容
- Fcitx5 wiki 工具默认抓取真实页面内容，过滤 MediaWiki 噪音
## [0.1.1] - 2026-06-29

### 🐛 Bug Fixes

- Linuxqq-appimage包名拼写错误
- Correct typos and formatting

### 💼 Other

- Nautilus或Thunar无法显示exe缩略图怎么办
- Pac pacr 命令的安装方法
## [0.1.0] - 2026-06-28
