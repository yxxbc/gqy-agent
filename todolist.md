## Main

- webui再safari上的移动端适配和界面固定，其次是移动端点击输入框整体界面不在上移动，或者考虑兼容方案，移动端浏览器：safari
- webui的提问和终端同步，不会再出现`错误: pending question not found`这种情况，复现：一个会话提出问题，终端的不点用webui点击会导致tui不同步【因为部分webui功能在tui中没有，但是项目没有对其进行处理的专属逻辑】。完成修复后看看其他的有没有此类相同问题（大部分是webui到tui的信息不同步。
- 修复“ 运行命令 · 5m 00s · 已中断 · gh pr checks 46 --repo SHORiN-KiWATA/miyu-agent -…
错误: LLM stream failed after emitting output; endpoint failover was suppressed: antigravity.stream transport failed (timeout): agy produced no output for 300s; the process was killed”这种情况，有些时候它要知道怎么使用后台任务，防止任务超时导致的整体任务丢失，当一个后台任务结束的时候可以主动告知顾清影，然后顾清影会知道并且优先清理干净其相关进程，然后继续处理任务。
- 修复 tui 中点击链接显示了正在打开的前端文字提示，但是后端没有反应正确打开链接的问题
- 顾清影每次回复不会太少,初期排查可能是`src/prompts`下`gqy.hint.md`的提示词问题
- agy 调用后的模型吞吐速度优化，实测下来通过顾清影的修复后换成3.7模型似乎比3.8快，具体改动查看 commit：`0e2690a`
- 修复webui 右侧工件清单 无法弹出问题，给顾清影添加一个控制这个的工具，让他生成文档或者预览文件的时候可以打开这个（目前推送交付区可以唤醒，可以二选一，推荐后者，不过后者需要优化，不然ai容易忘记）
- 给 顾清影安排一个专属 github 工具（功能：pr、commit、issue等都带末尾Co-Authored-By: 顾清影【模型名称】 (上下文长度) <noreply@https://github.com/yxxbc/gqy-agent>（已经完成，建议添加相关文档并删除此条）
- 寻找正确识别或者读取agy、claude、codex中token详细消耗的方法，去网上寻找答案
- 修复上下文圆环点击后的浮窗对应的数据要准确,增加agy、claude、codex的订阅时效额度，参考codexbar、(https://github.com/tungcorn/antigravity-usage-checker)、(https://github.com/skainguyen1412/antigravity-usage)、(https://github.com/phuryn/claude-usage)
- tui 美化意见
- webui 美化意见

## Feats

- 多发行版打包工作流，MacOS适配
- Live2D
- 支持QQ官方机器人
- 支持telegram
- 安全性、权限
- 首次使用TUI OOBE

## 优化

优化数据统计页面，提升可读性和美观度

减少token消耗

重写完整TUI

多平台字体处理

优化io性能

优化占用，提高运行效率和稳定性

减少dev模式AI看到的提示词，以让AI尽可能发挥自身的能力而不受影响

沙盒和webui文件分享、附件、上传功能的兼容性

agy 桥接工具瘦身：gqy_tools_eager=true 时全部工具定义被全量注入 MCP 条目，每轮固定吃约 9k input token 且随会话累积。改法：默认改走按需加载，gqy_tools 支持白名单数组，另给 MCP 出站 schema 加空 enum 兜底净化

## 搁置

- REPL 复制不带左侧装饰
