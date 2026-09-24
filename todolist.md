## Main

TUI 这一批（09-23 定，方案都在 `docs/plan/`）：

- 待验收：TUI 配色（`6f7f4c03` 等，09-24 `147c5a6a` 起 Linux/macOS 全绿，等用户手测）；CHANGELOG 与一键发版（`41591af3`，下次发版首次实跑）
- 待验收（09-24 已推送，本机已装无语音版）：打开方式 `-c`（`49e55909`）、圆角输入框（`d00ba353`）、开屏欢迎框与吉祥物（`a4826644`）、方向键选斜杠命令（`d157349d`）、`/config` 分组补全与直达（`e15dc60d`）。验收通过后 7–10 项写进 CHANGELOG `[Unreleased]`
- `gqy --banner` 说明文字过时（`args.rs`、`banner/preview.rs` 还写着星空），随 CHANGELOG 一起改
- TUI 验收问题 10 条，详情见 `docs/plan/2026-09-24-tui-acceptance-issues.md`：
  1. 空会话 `/goal` 底栏左右闪跳
  2. `/goal` 输出不留历史
  3. `/usage` 改浮窗
  4. `/config` 旧界面、分组藏得深
  5. 行内代码没渲染
  6. 欢迎框「最近」改碎碎念
  7. 空会话底栏先占 7.3k 上下文
  8. 黑猫吉祥物不显示
  9. 文件类输出点击不展开
  10. 输入框换行后点击光标乱跳、底栏变两行

详情写在文档里，这里一句话概括并引用。没另外写文档的条目，详情在 `docs/plan/backlog.md`（下面的「§N」）。

- 聊后复盘开机补跑：daemon 重启后没排上的复盘要补（§1）
- 纠正记忆写入时，把高度相似的旧错误说法标成「已否定」（§2）
- dev 模式下 Claude / agy / Codex 用各自的原生工具，WebUI 显示其工作区与会话（§3）

## Feats

- WebUI 适配 Safari 移动端，点输入框时页面不上移（§4）
- WebUI 回答提问后终端不同步（`pending question not found`），顺带查其他 WebUI→终端的同步问题（§5）
- 长命令 300 秒无输出被杀、整轮任务丢失；后台任务完成后主动通知顾清影（§6）
- 修复「聊天流式响应为空」报错（§7）
- agy 模型吞吐速度（3.7 似乎比 3.8 快，参考 `0e2690a`）（§8）
- WebUI 右侧工件清单弹不出来，给顾清影加控制它的办法（§9）
- 查清怎样读到 agy / Claude / Codex 的详细 token 消耗（§10）
- 上下文圆环浮窗数据要准，加上订阅额度（§11）
- WebUI 美化
- 外部扩展（脚本、MCP、技能、pm 包）进设置的插件列表，**方案待定**：`docs/plan/2026-09-24-extensions-in-plugin-list.md`
- WebUI 供应商显示彩色品牌图标：`docs/plan/2026-09-24-webui-provider-icons.md`
- Live2D
- 支持 QQ 官方机器人
- 支持 Telegram
- 安全性、权限
- macOS 沙盒后端（Seatbelt），现在 macOS 上绑了沙盒的会话命令一条都跑不了（§12）

## 优化

- 语音：唤醒调参、识别模型冷启动、供应商流式合成（§13）
- agy 桥接瘦身的后续：MCP schema 净化，用 cache-usage 做前后对比（§14）
- 拆分 `src/render/stream/timeline.rs`（2400 行，越过红线），下次改它时顺手拆（§15）
- 顾清影日常对话的反思机制：交稿前自查、纠正记忆、聊后复盘（§16，设计稿 `docs/design/2026-09-19-daily-chat-reflection.md`）
- 数据统计页面的可读性和美观度
- 减少 token 消耗
- 多平台字体处理
- IO 性能
- 降低占用，提高运行效率和稳定性
- 减少 dev 模式下 AI 看到的提示词
- 沙盒与 WebUI 文件分享、附件、上传的兼容性
