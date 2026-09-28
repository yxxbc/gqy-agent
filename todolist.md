## Main

- 聊后复盘开机补跑：daemon 重启后没排上的复盘要补（§1）
- 纠正记忆写入时，把高度相似的旧错误说法标成「已否定」（§2）
- dev 模式下 Claude / agy / Codex 用各自的原生工具，WebUI 显示其工作区与会话（§3；原生工具那半已做，WebUI 侧没做）
- 在前端中命令、工具调用的显示，展开详情显示的是类似一个卡片，上面写着【参数：】，下面写着【输出：】，二者之间有线隔开
- 会话并发，不再依赖单一daemon发送，高效处理并发能力（涉及底层修复）：
  - **现状根因**：当前底层所有不同客户端/平台（WebUI / CLI / QQ / Subagent）的会话请求全排进同一个全局单通道（`mpsc::UnboundedReceiver<ActorCommand>`）单线程串行处理。导致不同会话互相阻塞（Session A 跑耗时 LLM/工具时，Session B 与 WebUI 死等）、出站与推进强绑定在单个全局 actor 循环。
  - **核心改造方向**：下放为 Per-Session Actor / Task（按 `session_id` 路由 + 细粒度条带锁），使不同会话独立并行推进，解耦全局单一发送流。
  - **涉及全部相关文件与路径清单**：
    - **守护进程与 Actor 调度**：
      - `src/daemon.rs`（守护进程入口与生命周期）
      - `src/web/mod.rs`（通道创建、全局服务装配与 Actor 启动）
      - `src/web/actor/mod.rs`（核心改造点：全局单通道 `actor_loop` 与 `ActorCommand` 消费）
      - `src/web/actor/job_wake.rs`（子代理 / 定时任务完成后的唤醒排队机制）
      - `src/web/rooms/driver.rs`（多会话 / 房间驱动器）
    - **请求接入与事件分发**：
      - `src/web/ipc_server.rs`（CLI / TTY IPC 套接字服务与 `StartTurn` 派发）
      - `src/web/routes.rs`（WebUI HTTP / SSE 接口与请求分发）
      - `src/web/event_map.rs`（`EventHub` 多会话、多连接事件订阅与广播）
    - **会话数据库与状态隔离**：
      - `src/state/conversation_db/mod.rs`（SQLite 连接池、WAL 模式与事务隔离）
      - `src/state/conversation_db/turns.rs`（回合数据存储、原子写入与 `recover_stale_turns` 断点恢复）
      - `src/state/store_registry.rs`（分会话数据库路由 `stores.for_session`）
    - **Agent 回合执行引擎**：
      - `src/agent/mod.rs`（Agent 实例与会话上下文管理）
      - `src/agent/turn_loop/mod.rs`（单回合执行循环、LLM 流式处理与工具调度）
      - `src/agent/turn_loop/parallel.rs`（回合内多工具并发执行）
    - **平台多通道出站**：
      - `src/platforms/outbound.rs`（跨平台出站消息投递队列与发送管道）
      - `src/platforms/mod.rs`（各通讯平台适配器接入）
    - **架构规范与设计文档**：
      - `docs/wiki/10-系统设计与架构.md`（系统 Actor 与会话设计规范）
      - `docs/architecture.md`（架构分层依赖定义）

## Feats

- 长命令 300 秒无输出被杀、整轮任务丢失；后台任务完成后主动通知顾清影（§6；后台任务完成的通知已做，超时那半没做）
- 修复「聊天流式响应为空」报错（§7）
- WebUI 右侧工件清单弹不出来，给顾清影加控制它的办法（§9；工具与自动展开路径都在，弹窗行为待手测一次）
- 上下文圆环浮窗数据要准，加上订阅额度（§11；数据口径已做，agy / Claude / Codex 的订阅额度没做）
- WebUI 美化
- WebUI 供应商显示彩色品牌图标：`docs/plan/2026-09-24-webui-provider-icons.md`（09-26 已实现、待验收：24 个品牌图标内嵌，设置页卡片/模型池 + 模型菜单分节都在用）
- WebUI 模型切换与模型菜单改造（09-26 已实现、待验收）：模型切换从框内底栏搬到输入框下方信息行左端；菜单按供应商分节、节头带品牌图标、可展开收起，顶部加了过滤框
- 内置 Cline 中转供应商：`docs/design/2026-09-26-cline-relay-provider.md`（09-26 已实现、待验收：第四条 CLI 中转线；模型目录读 cline 自带的 `@cline/llms`，上下文窗口自动回填；续传核对不过自动退化全量重放）
- Live2D
- 支持 QQ 官方机器人
- 支持 Telegram（现在有了通用连接器协议，写个连接器即可，不用改 gqy）
- 安全性、权限
- macOS 沙盒后端（Seatbelt）：09-27 已做，见 `docs/design/2026-09-27-macos-sandbox.md`（§12 记录）

## 优化

- 语音：唤醒调参、识别模型冷启动、供应商流式合成（§13）
- agy 桥接瘦身的后续：MCP schema 净化，用 cache-usage 做前后对比（§14）
- 拆分 `src/render/stream/timeline.rs`（2400 行，越过红线），下次改它时顺手拆（§15）
- 顾清影日常对话的反思机制：交稿前自查、纠正记忆、聊后复盘（§16，设计稿 `docs/design/2026-09-19-daily-chat-reflection.md`；第一、二期已随 0.7.0 发布，第三期「交稿前自查」没做）
- 数据统计页面的可读性和美观度
- 减少 token 消耗
- 多平台字体处理
- IO 性能
- 降低占用，提高运行效率和稳定性
- 减少 dev 模式下 AI 看到的提示词
- 沙盒与 WebUI 文件分享、附件、上传的兼容性
