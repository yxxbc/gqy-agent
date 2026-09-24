# 待办详情

`todolist.md` 只放一句话概括，详细的现象、复现、方案写在这里（09-24 起的写法）。下面各条是当时待办的原文，照搬未改。

## 1. 聊后复盘开机补跑

聊后复盘开机补跑：复盘定时只在 daemon 内存里，15 分钟内关机或重启 daemon 就丢了，开机后不补，除非回到同一会话再说话。改法：daemon 启动时扫最近几天「最后一轮已过等待时长且没复盘过」的属主会话补排复盘（见 `docs/design/2026-09-19-daily-chat-reflection.md`）

## 2. 纠正记忆写入时处理冲突

纠正记忆写入时处理冲突：现在纠正是直接插入，旧的错误说法原样保留、可能和纠正一起被召回，只能等整理器碰巧改写。改法：存纠正时用记忆去重的语义相似度找高度相似的旧记录，把真值标成「已否定」（召回和整理器都会跳过），门槛设高避免误伤

## 3. dev 模式用 Claude / agy / Codex 原生工具

dev模式下，如果是claude、agy、codex，默认使用其原生工具，只使用gqy的anysearch工具，同时webui下显示其工作区项目，会话记录。webui输入框能显示选择客户端。供应商中的claude、codex、agy只能在人格模式下调用。参考：`https://github.com/makecindy/cindy/tree/main/packages/maker-core/src/agents`

## 4. WebUI 在 Safari 移动端的适配

webui再safari上的移动端适配和界面固定，其次是移动端点击输入框整体界面不在上移动，或者考虑兼容方案，移动端浏览器：safari

## 5. WebUI 与终端的提问同步

webui的提问和终端同步，不会再出现`错误: pending question not found`这种情况，复现：一个会话提出问题，终端的不点用webui点击会导致tui不同步【因为部分webui功能在tui中没有，但是项目没有对其进行处理的专属逻辑】。完成修复后看看其他的有没有此类相同问题（大部分是webui到tui的信息不同步。

## 6. 长命令超时丢任务，后台任务完成后通知

修复“ 运行命令 · 5m 00s · 已中断 · gh pr checks 46 --repo SHORiN-KiWATA/miyu-agent -…
错误: LLM stream failed after emitting output; endpoint failover was suppressed: antigravity.stream transport failed (timeout): agy produced no output for 300s; the process was killed”这种情况，有些时候它要知道怎么使用后台任务，防止任务超时导致的整体任务丢失，当一个后台任务结束的时候可以主动告知顾清影，然后顾清影会知道并且优先清理干净其相关进程，然后继续处理任务。

## 7. 「聊天流式响应为空」报错

修复错误: LLM stream failed after emitting output; endpoint failover was suppressed: 聊天流式响应为空

## 8. agy 模型吞吐速度

agy 调用后的模型吞吐速度优化，实测下来通过顾清影的修复后换成3.7模型似乎比3.8快，具体改动查看 commit：`0e2690a`

## 9. WebUI 工件清单弹不出来

修复webui 右侧工件清单 无法弹出问题，给顾清影添加一个控制这个的工具，让他生成文档或者预览文件的时候可以打开这个（目前推送交付区可以唤醒，可以二选一，推荐后者，不过后者需要优化，不然ai容易忘记）

## 10. 读取 agy / Claude / Codex 的详细 token 消耗

寻找正确识别或者读取agy、claude、codex中token详细消耗的方法，去网上寻找答案

## 11. 上下文圆环浮窗的数据与订阅额度

修复上下文圆环点击后的浮窗对应的数据要准确,增加agy、claude、codex的订阅时效额度，参考codexbar、(https://github.com/tungcorn/antigravity-usage-checker)、(https://github.com/skainguyen1412/antigravity-usage)、(https://github.com/phuryn/claude-usage)

## 12. macOS 沙盒后端

macOS 沙盒后端：`/sandbox` 目前只有 Linux 的 Landlock 后端（`src/tools/sandbox/linux.rs`），非 Linux 走 `unsupported.rs`——**失败关闭**：绑了沙盒的会话在 macOS 上命令一条都跑不了（不是直通裸奔）。补 `src/tools/sandbox/macos.rs`，把 `SandboxPolicy` 译成 Seatbelt profile 交给 `sandbox-exec`：放行工作区、临时目录与 npm/cargo/pip 编译缓存，挡掉 `~/.ssh`、`~/.gnupg`、`~/.aws` 等凭证目录；需要越权的命令走确认或降级直通。（miyu-agent#47）

## 13. 语音优化（09-16 盘点）

语音（09-16 盘点，此前所有语音提交都是功能接入与修复，没做过优化）：

- 唤醒调参：`wake_threshold` / `wake_boost` 现在是拍脑袋的默认值，没有误触发率与漏触发率的实测底子。要先攒一批真实录音做回归集，再谈调参。
- SenseVoice 加载抖动：`stt_unload_seconds` 到点卸载后，下一句要吃 1.2~2s 冷启动。可以按"刚说完话的一小段时间内不卸载"改成滑动续期，或在唤醒命中的瞬间就预热。
- 供应商流式合成：MiniMax `t2a_v2` 与 MiMo 都支持 `stream`，当前两家都写死 `"stream": false`。接上能把首声延迟再压一截，但要处理两家不同的分块格式与 wav 拼接。

## 14. agy 桥接工具瘦身的后续

agy 桥接工具瘦身：09-24 已做常用工具白名单（内置约 22 个常驻，其余懒加载，用户可加 `gqy_tools_eager_extra`）与用量口径修正（按每次调用求和）。实测原来每次模型调用约背 1.7–1.8 万 token 的完整说明。剩下：给 MCP 出站 schema 加空 enum 兜底净化；等用量口径修好后跑几天，拿 cache-usage 前后对比实际省了多少

## 15. 拆分 `src/render/stream/timeline.rs`

拆分 `src/render/stream/timeline.rs`（2400 行，唯一越过 2000 行红线的文件，AGENTS §6.2）：目前体验没问题、近期改动也很少碰它，暂不排期；等下次要改它时顺手拆

## 16. 顾清影日常对话的反思机制

顾清影日常对话的反思机制（纯文本聊天为主，dev 模式另说）：

- 起因案例（09-19）：她写的 ClinePass 争议报告有硬伤——截图里 13 次请求写成 16 次、「40 美元额度」无出处、把用户没说过的论点写进用户主张、把对方「按限额百分比推算」写成「按价格推算」。纠错发生在群聊和 Claude Code 里，她没有渠道得知，事后还邀功说「每个细节和证据都打理得妥妥帖帖」。
- 机制一，交稿前自查：只在写报告/复盘/核查这类事实性内容时触发，拿结论逐条对照手头原始材料（截图、聊天记录），标出对不上的、没出处的、没人说过的。目标是说得更准而不是说得更多，不能变成事事免责的客服腔。
- 机制二，纠正记忆：识别用户的纠正（「不是这样」「我说过」「报告有错」），连同原因存成单独一类记忆，相似场景召回。
- 机制三，聊后复盘：一段对话结束后发独立辅助请求（§1.7 独立缓存），检查读错情绪、附和、无据断言、对未核实的工作打包票；结论写成「这次注意什么」，下一轮从 system 侧注入（§1.4 不化石），不漏进她的台词。
- 风险：过度道歉、反思变表演（OOC）、人格变拘谨。人格文本是实测敏感区（§1.5），改动要小步实测。
- 验收（原样重放起因案例）：① 给同样的聊天记录和截图让她写报告——次数写 13、40 美元不出现或标成假设、不替人补论点；② 告诉她报告有错——她会记下来；③ 几轮后再提这份报告——她承认算错而不是邀功，撒娇可以照旧
