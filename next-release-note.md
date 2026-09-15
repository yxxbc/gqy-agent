>更新内容记录在此处，每次更新release时作为releasenote发布，发布后清理已发布内容。

## 顾清影定制版 (0.6.0)
- 适配 macOS 麦克风 48kHz 重采样抗混叠与 ALSA 过滤门控，修复 KWS 唤醒词不命中
- 内置图库 (`album`)、高德/OSM 地图 (`map`)、快递 100 查询 (`express`)
- WebUI 新增上下文占用分项拆解面板、划词助手、围栏图表 (Mermaid) 预览与成果交付增强
- 后台任务与会话多用户安全隔离访问收口
- 新增 `github` 工具：commit、提 PR / issue、评论与 gh 直通。你固定是 author，顾清影 自动挂 `Co-Authored-By: 顾清影【模型】 (上下文窗口)`；名字与邮箱可在 `tools.github` 配置，`gqy reload` 生效
- 顾清影 可以有自己的 GitHub 账号：`gqy github login / status / logout`，凭据隔离在 `~/.gqy/github/`，不碰你的 gh / git 登录与钥匙串；默认用你的身份，明确要求时才用她的账号，推不进的仓库自动 fork 后提 PR
- 修复 bot 身份下 gh 找不到自己的 token 时会回退使用宿主钥匙串 token 的隐患：没有 bot token 一律拒绝执行
