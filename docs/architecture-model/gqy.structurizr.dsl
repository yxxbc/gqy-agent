workspace "顾清影 gqy — 当前架构 (as-built)" "Current-state model, 2026-09-26, system-modeler + c4model. Evidence index: gqy.evidence.md. Dense module map: layers.dot. Scope: 代码现状，不含目标架构。" {

  model {

    // ── L1 people ──────────────────────────────────────────────
    owner = person "属主 / 管理员" "终端前的人，也是第一个 WebUI 账号。Owner 信任级，回合默认不套沙盒。" { tags "Person" }
    member = person "成员账号" "邀请码注册。只有自己的 conversation.db、工作区和私有 persona。Member 信任级，回合套 Landlock。" { tags "Person" }
    outsider = person "聊天对面的第三方" "QQ / iMessage 会话里的昵称与正文，全部是不可信输入（过 safe_prompt_field）。" { tags "Person,Untrusted" }

    // ── L1 external systems ────────────────────────────────────
    onebot = softwareSystem "OneBot v11 端" "QQ 协议实现。连进 daemon 的 /ws 或 /onebot/v11/ws；配置了不同端口时 daemon 另开一个监听。" { tags "External" }
    imsgBridge = softwareSystem "iMessage 桥" "scripts/imessage/ 里的 Python LaunchAgent，读 macOS 信息库，用 gqy ask --session imessage-* 进话。路径被 install.sh 编译进启动器，独立于 gqy 发布。" { tags "External,Bridge" }
    llmProvider = softwareSystem "上游模型供应商" "OpenAI 兼容 / Anthropic / 中转线三族协议；四档池 Lite · Cheap · Standard · Flagship。" { tags "External" }
    mcpServers = softwareSystem "MCP 服务器" "config.mcp.servers 声明的外部进程（id/command/args/env/timeout）。stdio 起子进程。" { tags "External" }
    ttsProvider = softwareSystem "播报供应商" "MiniMax t2a_v2（hex wav）或小米 MiMo chat/completions（base64 wav）。" { tags "External" }
    webNet = softwareSystem "公网" "web_search / web_fetch 的目标，以及 AUR、GitHub API。" { tags "External" }
    toolchain = softwareSystem "本机工具链" "按 PATH 找的外部命令：sh、rg、chafa、git、gh、pacman/makepkg、wl-copy/xclip、notify-send、pactl、fcitx5-remote、$EDITOR、中转线 CLI（claude / codex / agy）。" { tags "External" }

    // ── the system ─────────────────────────────────────────────
    gqy = softwareSystem "顾清影 (gqy)" "单个 Rust crate、autobins=false。除 gqy-voice 外所有形态共用一个二进制。" {
      tags "System"

      cli = container "gqy CLI" "前台入口与一次性客户端：REPL、gqy ask、shellhook、stdio 宿主协议、gqy web、daemon 启停、config_tui / question_tui、OOBE、mcp-serve。自己不跑回合，一律经 IPC 交给 daemon。" "Rust · clap · tokio" { tags "Process,Entry" }
      daemon = container "gqy __daemon" "唯一的常驻进程，也是唯一跑回合的地方。axum HTTP/SSE + OneBot 反向 WS + unix socket IPC + 单 actor 准入。默认 0.0.0.0:8300。" "Rust · axum · rusqlite" {
        tags "Process,Daemon"

        // ── L3 components inside the daemon ──────────────────
        surface = component "场所层 web/server + ipc_server" "HTTP 路由、SSE /api/events、unix socket core.sock 帧协议、tty、sandbox_scope。" "Rust" { tags "Surface" }
        admission = component "单 actor 准入" "一个 mpsc 收 ActorCommand；StartTurn 逐个 spawn_local。串行的是准入不是执行——同会话在队列里排，跨会话并发。" "Rust" { tags "Core" }
        turnLoop = component "回合引擎 agent/turn_loop" "模型↔工具循环；控制闸（一会话一回合，Drop 型守卫）；只有 subagent 一个工具内部并行，按请求序回填结果。" "Rust" { tags "Core" }
        promptAsm = component "系统提示词组装" "骨架 + 人格全文 + 属主档案 + 子系统前言 + host-environment 能力位。每请求重拼，不化石。" "Rust" { tags "Core" }
        ctxFossil = component "上下文与化石回放" "append-only：turn_context_messages / journal 逐字节重放；逐出前先落盘工具输出再压缩。" "Rust" { tags "Core" }
        toolFace = component "工具目录 compose + registry" "UNITS 表 34 条按 persona 门控注册；descriptions/*.json 整体覆盖 Rust 占位；stub 档位下只露真名 + ≤60 字摘要 + 空参数壳。" "Rust + JSON" { tags "Capability" }
        toolExec = component "工具分发与闸" "按 schema 收口畸形参数；trust 位过滤；requires_prior 跨工具闸；复读闸；命令拒绝串。" "Rust" { tags "Capability" }
        sandbox = component "Landlock 沙盒" "成员回合与 /sandbox 绑定的会话锁读写集；进程内 guard_read/guard_write 先查一遍；rg 与中转线 CLI 也关进去。非 Linux 平台 fail-closed。" "Rust + Landlock" { tags "Security" }
        llmClient = component "模型池 llm/" "OpenAI 兼容 / Anthropic / 中转线；池优先级 回合覆盖 > 平台引用 > 全局；前缀逐字节稳定吃缓存；cache-usage 日志。" "Rust" { tags "Core" }
        stores = component "StoreRegistry 身份路由" "admin store 加 members/owners 两张表；principal = blake3(入口,账号,用户 id) 取 24 hex，随会话冻结。Web/actor/IPC 都按会话所属 store 走。" "Rust" { tags "State" }
        memorySub = component "记忆子系统" "三个挂接点：A 联想注入、B 逐出库归档、C 回合后写事实/经历/日记。" "Rust" { tags "Subsystem" }
        platformAdapt = component "平台适配 platforms/" "OneBot 分发与回复、turn_context 解析、access control、群管、赞助特效、scheduled_messages、real_context（情绪/好感度）。" "Rust" { tags "Subsystem" }
      }
      voice = container "gqy-voice" "语音前端：麦克风、唤醒词、VAD、SenseVoice 本地识别、提示音与播报播放。只有 --features voice 才构建，sherpa-onnx 永不进主二进制。" "Rust · sherpa-onnx · cpal" { tags "Process,Optional" }
      rendererW = container "gqy __renderer-worker" "长图渲染子进程（Markdown→PNG）。env GQY_INTERNAL_RENDERER_WORKER=1 触发，空闲 10 分钟退出；RLIMIT_AS 512MB（debug 2GB）仅 Linux 生效，macOS 上 setrlimit 回 EINVAL 故不设。" "Rust" { tags "Process,Worker" }
      embedW = container "gqy __embedding-worker" "本地向量模型子进程，ONNX 动态库外置以免 24MB 常驻在 daemon。env GQY_INTERNAL_EMBEDDING_WORKER=1，空闲 600 秒退出，起不来冷却 5 分钟；RLIMIT_AS 1GB（debug 4GB）同样仅 Linux。" "Rust · ort" { tags "Process,Worker" }
      alarmW = container "gqy __alarm-worker" "闹钟 worker，detached 且故意活得比 gqy 长。" "Rust" { tags "Process,Worker" }

      convDb = container "conversation.db" "每个身份一份。turns / sessions / app_state / question_exchanges / image_assets / queued_prompts / artifact_assets / platform_access_grants / journal / shared_files / accounts / invites / turn_tool_reports / session_reviews。PRAGMA user_version 到 37。" "SQLite (bundled)" { tags "Database,PerIdentity" }
      memDb = container "memory.db" "每个 persona 一份。facts / episodes / pending_events / skill_records / memory_revisions / memory_meta / memory_embeddings，带 visibility 与 owner_principal 列。" "SQLite" { tags "Database,PerPersona" }
      evictDb = container "evicted_context.db" "压缩逐出回合的归档，挂接点 B 写入。" "SQLite" { tags "Database" }
      ledgerDb = container "ledger.db" "记账扩展自有库，自己的 user_version，不走 MIGRATIONS。" "SQLite" { tags "Database" }
      kbDb = container "kb_meta.db · semantic_index.db" "知识库文件表与语义分块表。机器级，不按 persona 切。" "SQLite" { tags "Database,Global" }
      flatState = container "state/ 与 cache/ 平面文件" "usage.json、usage-history.jsonl、web-sessions.json（sha256 令牌 0600 30 天）、daemon-launch.json、web-passwords/、thinking-variants.json、relay/sessions.json、prompt-fingerprints/、models_cache.json、provider-capabilities.json、cache-usage.*.jsonl。" "JSON · JSONL" { tags "FileStore" }
      homeTree = container "~/.gqy 目录树" "config ≈/etc、personas ≈/usr/share、extensions ≈/usr/lib、data、home/<user>。规则：用户产生的进 home，管理员发布的在根且成员只读，机器运行态在 state/cache。" "Filesystem" { tags "FileStore" }
    }

    // ── L1→L2 relationships ────────────────────────────────────
    owner -> cli "终端对话、改配置、起停 daemon"
    owner -> daemon "浏览器开 WebUI（:8300）"
    member -> daemon "登录 WebUI；只看自己名下的会话"
    outsider -> onebot "发消息、收图、点表情"
    outsider -> imsgBridge "macOS 信息 App 里的对话"

    cli -> daemon "core.sock 一次连接一个回合（IPC 帧协议）" "unix socket"
    voice -> daemon "VoiceAttach 挂进同一 socket" "unix socket"
    daemon -> voice "spawn 并把 PCM / 播放任务递过去" "child process" { tags "Optional" }
    daemon -> rendererW "spawn 渲长图" "child process"
    daemon -> embedW "spawn 算向量" "child process"
    daemon -> alarmW "spawn（detached，闹钟到点触发 job-wake）" "child process"
    imsgBridge -> cli "gqy ask --session imessage-* --output-format json" "CLI"
    onebot -> daemon "反向 WebSocket 连入 /ws、/onebot/v11/ws" "WebSocket"
    daemon -> llmProvider "调模型：流式补全 + 工具调用" "HTTPS" { tags "TrustBoundary" }
    daemon -> ttsProvider "合成播报音频" "HTTPS"
    daemon -> mcpServers "起 stdio 子进程、tools/list" "stdio"
    daemon -> toolchain "起子进程干活；成员回合下整个子进程继承 Landlock 规则" "exec"
    daemon -> webNet "web_search / web_fetch / AUR / GitHub API" "HTTPS"

    daemon -> convDb "读写回合、会话、账户" "SQL"
    daemon -> memDb "读写事实、经历、日记、嵌入" "SQL"
    daemon -> evictDb "逐出回合归档" "SQL"
    daemon -> ledgerDb "记账扩展自有读写" "SQL"
    daemon -> kbDb "知识库文件与分块" "SQL"
    daemon -> flatState "用量、令牌、daemon 启动配置、模型能力缓存" "file"
    daemon -> homeTree "persona / extensions / workspace / 沙盒根" "file"
    cli -> homeTree "首启读 config、跑 OOBE" "file" { tags "Inferred" }

    // ── L3 relationships (inside daemon) ───────────────────────
    surface -> admission "提交一个回合"
    admission -> turnLoop "spawn_local 起回合"
    turnLoop -> promptAsm "第 3 步组系统提示词"
    turnLoop -> ctxFossil "第 4 步组历史"
    turnLoop -> toolFace "取当前工具面"
    turnLoop -> llmClient "第 6 步调模型"
    turnLoop -> toolExec "第 7 步分发工具调用"
    toolExec -> toolFace "definitions / contracts / stub 判定"
    toolExec -> sandbox "路径守卫，并把子进程关进沙盒"
    toolExec -> stores "读写会话与产物"
    turnLoop -> memorySub "挂接点 A 联想注入 / C 回合后钩子"
    ctxFossil -> memorySub "挂接点 B 逐出库归档"
    promptAsm -> stores "persona 与属主档案"
    surface -> stores "按会话所属 store 路由"
    platformAdapt -> admission "平台侧 StartTurn（带发送者与平台池引用）"
    platformAdapt -> stores "principal 解析与会话归属"
    llmClient -> toolFace "中转线复用工具契约" { tags "CrossLayer" }
  }

  views {
    systemContext gqy "L1-SystemContext" "边界视图：三个人、七个外部系统、一个软件系统" {
      include *
      autolayout lr
    }

    container gqy "L2-Containers" "进程与存储：谁是什么形态的 OS 进程，数据落在哪个文件" {
      include *
      autolayout lr
    }

    component daemon "L3-TurnPipeline" "daemon 内部：一回合从入口到落库经过的模块" {
      include *
      autolayout lr
    }

    styles {
      element "Person" {
        shape person
        background #083F77
        color #ffffff
      }
      element "Untrusted" { background #C0392B }
      element "External" {
        shape hexagon
        background #999999
        color #ffffff
      }
      element "System" {
        shape components
        background #0F635F
        color #ffffff
      }
      element "Process" {
        shape roundedBox
        background #116953
        color #ffffff
      }
      element "Daemon" { background #083F77 }
      element "Entry" { background #2E86C1 }
      element "Worker" {
        background #7D6608
        fontSize 12
      }
      element "Optional" { border #D68910 }
      element "Database" {
        shape cylinder
        background #512D81
        color #ffffff
      }
      element "FileStore" {
        shape folder
        background #6E2C00
        color #ffffff
      }
      element "Surface" { background #2E86C1 }
      element "Core" { background #083F77 }
      element "Capability" { background #116953 }
      element "Subsystem" { background #7D3C98 }
      element "State" { background #B9770E }
      element "Security" { background #943126 }
      element "Inferred" {
        border #D68910
        opacity 70
      }
      relationship "TrustBoundary" {
        color #C0392B
        thickness 2
      }
      relationship "CrossLayer" {
        color #D68910
        dashed true
      }
      relationship "Optional" {
        color #7F8C8D
        dashed true
      }
    }

    default {
      fontSize 14
      orientation leftToRightFromTop
    }
  }
}
