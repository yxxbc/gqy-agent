//! daemon 侧的语音桥:进程看护 + 信令中继 + 语音回合驱动。
//!
//! **不含任何语音栈代码,不吃 voice feature**——识别模型/麦克风/提示音都在
//! 独立的 `gqy-voice` 进程里(见 `voice::worker` 的信令表)。这里负责:
//!
//! - 定位并拉起/看护 `gqy-voice`(崩溃退避重启,daemon 关闭时收走,
//!   配置重载时按需重启);
//! - 持有它的 `VoiceAttach` 信令连接;
//! - 唤醒路:`voice.command` → 在「语音会话」lane 起回合 → 完成后桌面通知
//!   回复摘要 + 提示音;进行中再开口即取消(打断);
//! - 听写中继:REPL `/stt`、`gqy stt` 认领听写流(本机麦),识别文本流回;
//!   WebUI 麦克风按钮同样认领一条听写流,但音频由浏览器经 WebSocket 推来
//!   (`push_audio` → worker 的 `voice.audio`),VAD/分句/识别仍在 worker;
//! - 整段录音转写(`/api/voice/transcribe`,外部脚本用)的请求/应答配对;
//! - `end_voice_chat` 工具的关窗钩子。
//!
//! 语音关闭时这里没有任何常驻状态:零线程、零内存、零占用。

use crate::runtime::DaemonState;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;

/// worker→daemon 的听写事件(转发给认领的客户端)。
pub(crate) enum DictationRelay {
    Utterance(String),
    Ended,
}

/// daemon→worker 的待发信令帧(kind, data)。
type Signal = (&'static str, Value);

pub(crate) const BINARY_NAME: &str = "gqy-voice";
const SESSION_ID_FILE: &str = "voice-session-id";
const WORKER_LOG: &str = "voice-worker.log";

static WORKER_PID: AtomicU32 = AtomicU32::new(0);
static SUPERVISOR_RUNNING: AtomicBool = AtomicBool::new(false);
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);
/// 配置重载要求 worker 退出后不再重启(语音被关掉)。
static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);
/// worker 的信令发送端(VoiceAttach 连接的写半边)。
static ATTACH: Mutex<Option<UnboundedSender<Signal>>> = Mutex::new(None);
/// worker 报上来的采集设备描述。
static DEVICE: Mutex<Option<String>> = Mutex::new(None);
/// 当前听写认领者的转写接收端。
static DICTATION: Mutex<Option<UnboundedSender<DictationRelay>>> = Mutex::new(None);
/// 进行中的语音回合 run_id(打断用)。
static ACTIVE_RUN: Mutex<Option<String>> = Mutex::new(None);
/// 「在听」通知的代数:唤醒后延迟一小段再弹,期间若同一口气里的指令已到
/// (voice.command 使代数失效)就只弹「收到」,不连弹两条。
static WAKE_NOTICE_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// 语音会话"代":快捷键把窗口关掉一次加一。回合在跑/在合成时被这样关掉,
/// 回合完成后的通知与播报一律作废,免得关了还念。
static VOICE_WINDOW_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// hold 的代际。hold 压着追问窗口的计时,而「谁开的谁关」在并发下不成立:
/// 上一轮被打断后返回 cancelled,它那句 hold off 会把**新一轮**刚压下去的
/// hold 掀掉,于是新一轮还在推理,30 秒追问窗口就已经开始倒计时了。只有仍是
/// 最新一代的回合才准放 hold。
static HOLD_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
const WAKE_NOTICE_DELAY_MS: u64 = 1200;
/// 浏览器录音转写的在途请求。
static TRANSCRIBES: Mutex<Option<HashMap<String, oneshot::Sender<Result<String, String>>>>> =
    Mutex::new(None);
/// 桌面通知的专用线程入口:一串语音通知按序发,Linux 上用 notify-send 的
/// 替换 id 让「在听」「收到」回复三条共用一个气泡(09-05:此前「在听」为了
/// 不和「收到」连弹被压了 1.2s,提示音却是即刻响的,用户听到音效看不到通知)。
static NOTIFY_QUEUE: Mutex<Option<std::sync::mpsc::Sender<(String, String)>>> = Mutex::new(None);

/// 找 `gqy-voice`:先看主程序同目录,再扫 PATH。
pub(crate) fn locate_binary() -> Option<PathBuf> {
    if let Ok(exe) = crate::paths::gqy_executable() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(BINARY_NAME);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(BINARY_NAME))
        .find(|candidate| candidate.is_file())
}

pub(crate) fn worker_running() -> bool {
    ATTACH.lock().unwrap().is_some()
}

/// `/api/voice/status` 与 IPC `VoiceStatus` 共用的状态快照。
pub(crate) fn status(state: &DaemonState) -> Value {
    let (enabled, tts_enabled, models_dir) = {
        let manager = state.manager.lock().unwrap();
        (
            manager.config.voice.enabled,
            manager.config.voice.tts.is_active(),
            state.paths.state_dir.join("models"),
        )
    };
    let binary = locate_binary();
    json!({
        "enabled": enabled,
        "tts": tts_enabled,
        "binary": binary.as_ref().map(|path| path.display().to_string()),
        "spawned": WORKER_PID.load(Ordering::Relaxed) != 0,
        "attached": worker_running(),
        "device": DEVICE.lock().unwrap().clone(),
        "dictating": DICTATION.lock().unwrap().is_some(),
        "models_dir": models_dir.display().to_string(),
        "log": state.paths.logs_dir().join(WORKER_LOG).display().to_string(),
    })
}

/// daemon 启动时调用:语音开启才拉 worker。
/// 语音唤醒或文本转语音任一开启都需要前端进程。
fn worker_wanted(voice: &crate::config::VoiceConfig) -> bool {
    voice.enabled || voice.tts.enabled
}

pub(crate) fn spawn_if_enabled(state: &DaemonState) {
    let wanted = worker_wanted(&state.manager.lock().unwrap().config.voice);
    if wanted {
        ensure_worker(state);
    }
}

/// 配置重载:voice 节变了就让 worker 重启(关掉则收走不再拉)。
pub(crate) fn on_config_reload(
    state: &DaemonState,
    previous: &crate::config::VoiceConfig,
    next: &crate::config::VoiceConfig,
) {
    let previous_json = serde_json::to_value(previous).unwrap_or(Value::Null);
    let next_json = serde_json::to_value(next).unwrap_or(Value::Null);
    if previous_json == next_json {
        return;
    }
    if worker_wanted(next) {
        // 让看护循环用新配置重启:worker 收到 SIGTERM 退出,循环立刻重拉。
        if WORKER_PID.load(Ordering::Relaxed) != 0 {
            kill_worker();
        } else {
            ensure_worker(state);
        }
    } else {
        STOP_REQUESTED.store(true, Ordering::Relaxed);
        kill_worker();
    }
}

/// daemon 优雅关闭:杀掉 worker,不再重启。
pub(crate) fn shutdown() {
    SHUTTING_DOWN.store(true, Ordering::Relaxed);
    kill_worker();
}

fn kill_worker() {
    let pid = WORKER_PID.load(Ordering::Relaxed);
    if pid != 0 {
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
    }
}

/// end_voice_chat 工具钩子:向 worker 发关窗信令。worker 不在时无操作
/// (非 daemon 进程里调用同样安全)。
pub(crate) fn end_voice_chat_hook() {
    send_signal("voice.close_window", json!({}));
}

fn send_signal(kind: &'static str, data: Value) {
    if let Some(sender) = ATTACH.lock().unwrap().as_ref() {
        let _ = sender.send((kind, data));
    }
}

/// 拉起 worker(已在跑则无操作),并启动看护任务。
pub(crate) fn ensure_worker(state: &DaemonState) {
    STOP_REQUESTED.store(false, Ordering::Relaxed);
    if WORKER_PID.load(Ordering::Relaxed) != 0 {
        return;
    }
    if SUPERVISOR_RUNNING.swap(true, Ordering::Relaxed) {
        return;
    }
    let state = state.clone();
    tokio::spawn(async move {
        let mut backoff_secs = 1u64;
        loop {
            if SHUTTING_DOWN.load(Ordering::Relaxed) || STOP_REQUESTED.load(Ordering::Relaxed) {
                break;
            }
            let Some(binary) = locate_binary() else {
                tracing::warn!(
                    "语音已启用但找不到 {BINARY_NAME} 可执行文件(主程序同目录或 PATH),语音功能停用"
                );
                break;
            };
            let log_path = state.paths.logs_dir().join(WORKER_LOG);
            let log = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
                .ok();
            let mut command = tokio::process::Command::new(&binary);
            command.stdin(std::process::Stdio::null());
            if let Some(log) = log {
                let err = log.try_clone().ok();
                command.stdout(std::process::Stdio::from(log));
                if let Some(err) = err {
                    command.stderr(std::process::Stdio::from(err));
                }
            }
            let mut child = match command.spawn() {
                Ok(child) => child,
                Err(error) => {
                    tracing::error!("语音前端启动失败({}): {error}", binary.display());
                    break;
                }
            };
            let pid = child.id().unwrap_or(0);
            WORKER_PID.store(pid, Ordering::Relaxed);
            tracing::info!("语音前端已启动(pid {pid},{})", binary.display());
            let started = std::time::Instant::now();
            let status = child.wait().await;
            WORKER_PID.store(0, Ordering::Relaxed);
            ATTACH.lock().unwrap().take();
            DEVICE.lock().unwrap().take();
            ACTIVE_RUN.lock().unwrap().take();
            if let Some(sink) = DICTATION.lock().unwrap().take() {
                let _ = sink.send(DictationRelay::Ended);
            }
            fail_all_transcribes("语音前端退出");
            if SHUTTING_DOWN.load(Ordering::Relaxed) || STOP_REQUESTED.load(Ordering::Relaxed) {
                break;
            }
            // 跑得久说明不是启动即崩,重置退避。
            if started.elapsed() > std::time::Duration::from_secs(60) {
                backoff_secs = 1;
            }
            tracing::warn!("语音前端退出({status:?}),{backoff_secs}s 后重启");
            tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
            backoff_secs = (backoff_secs * 2).min(60);
            let wanted = worker_wanted(&state.manager.lock().unwrap().config.voice);
            if !wanted {
                break;
            }
        }
        SUPERVISOR_RUNNING.store(false, Ordering::Relaxed);
    });
}

/// VoiceAttach 处理:worker 注册信令连接。写半边发送 daemon 侧信令,
/// 读半边接收 worker 事件并分发。
pub(crate) async fn handle_voice_attach(
    state: &DaemonState,
    stream: &mut tokio::net::UnixStream,
) -> Result<()> {
    let (tx, mut rx): (UnboundedSender<Signal>, UnboundedReceiver<Signal>) = unbounded_channel();
    *ATTACH.lock().unwrap() = Some(tx);
    crate::ipc::send(stream, &crate::ipc::Frame::Ack).await?;
    loop {
        tokio::select! {
            signal = rx.recv() => {
                let Some((kind, data)) = signal else { break };
                crate::ipc::send(stream, &crate::ipc::Frame::Event { id: 0, kind: kind.to_string(), data }).await?;
            }
            frame = crate::ipc::receive::<crate::ipc::Frame>(stream) => {
                let Ok(Some(crate::ipc::Frame::Event { kind, data, .. })) = frame else {
                    break; // worker 断开
                };
                handle_worker_event(state, &kind, data);
            }
        }
    }
    ATTACH.lock().unwrap().take();
    DEVICE.lock().unwrap().take();
    if let Some(sink) = DICTATION.lock().unwrap().take() {
        let _ = sink.send(DictationRelay::Ended);
    }
    fail_all_transcribes("语音前端断开");
    Ok(())
}

fn text_field(data: &Value, key: &str) -> String {
    data.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn handle_worker_event(state: &DaemonState, kind: &str, data: Value) {
    use crate::i18n::text as t;
    match kind {
        "voice.ready" => {
            *DEVICE.lock().unwrap() = Some(text_field(&data, "device"));
        }
        "voice.wake" => {
            // 唤醒只是「我要开口了」,指令还没到:掐嘴不掐活。真要换指令,下面
            // voice.command 那条自会取消上一轮。
            send_signal("voice.stop_speaking", json!({}));
            let generation = WAKE_NOTICE_GEN.fetch_add(1, Ordering::Relaxed) + 1;
            if cfg!(target_os = "linux") {
                // 提示音在前端即刻响,通知也即刻弹,和音效同步;同口气带指令时
                // 「收到」会替换掉这条(notify-send -r),不会连弹两条。
                notify(
                    state,
                    t("Selene is listening", "顾清影 在听"),
                    t("speak now", "请讲"),
                );
            } else {
                // 没有替换能力的平台:延迟一点,同口气带指令时只弹「收到」。
                let state = state.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(WAKE_NOTICE_DELAY_MS))
                        .await;
                    if WAKE_NOTICE_GEN.load(Ordering::Relaxed) == generation {
                        notify(
                            &state,
                            t("Selene is listening", "顾清影 在听"),
                            t("speak now", "请讲"),
                        );
                    }
                });
            }
        }
        "voice.speech_start" => {
            // 起势人声:让她闭嘴,但**别动正在跑的回合**。以前这里直接取消,于是
            // 她在改文件时你插一句「等一下」,活就白干了(GPT-Live 的同款结论:
            // 打断语音不该连带取消后台工作)。明确要停有两条路:说新指令(走
            // voice.command)、或按快捷键(走 reason=listen)。
            send_signal("voice.stop_speaking", json!({}));
        }
        "voice.command" => {
            let text = text_field(&data, "text");
            if text.is_empty() {
                return;
            }
            WAKE_NOTICE_GEN.fetch_add(1, Ordering::Relaxed);
            cancel_active_run(state);
            notify(state, t("Selene heard", "顾清影 收到"), &clip(&text, 80));
            let state = state.clone();
            tokio::spawn(async move {
                if let Err(error) = run_voice_turn(&state, text).await {
                    tracing::error!("语音回合失败: {error:#}");
                    notify(
                        &state,
                        t("GQY voice error", "语音会话出错"),
                        &clip(&format!("{error:#}"), 120),
                    );
                    send_signal("voice.cue", json!({ "name": "error" }));
                }
            });
        }
        "voice.dictation" => {
            let text = text_field(&data, "text");
            if let Some(sink) = DICTATION.lock().unwrap().as_ref() {
                let _ = sink.send(DictationRelay::Utterance(text));
            }
        }
        "voice.window_closed" | "voice.timeout" => {
            // 听写窗口静默结束 → 通知认领者;唤醒对话窗口关闭不打扰。
            if let Some(sink) = DICTATION.lock().unwrap().take() {
                let _ = sink.send(DictationRelay::Ended);
            }
            // 快捷键再按一次关掉的:掐回合、作废还没播的播报、给个"不听了"的
            // 反馈。静默超时不打扰;模型自己调 end_voice_chat 关窗时告别语照念。
            if text_field(&data, "reason") == "listen" {
                VOICE_WINDOW_GEN.fetch_add(1, Ordering::Relaxed);
                WAKE_NOTICE_GEN.fetch_add(1, Ordering::Relaxed);
                cancel_active_run(state);
                send_signal("voice.stop_speaking", json!({}));
                notify(state, "顾清影", t("stopped listening", "不听了"));
            }
        }
        "voice.transcribed" => {
            let request_id = text_field(&data, "request_id");
            let text = text_field(&data, "text");
            let error = data
                .get("error")
                .and_then(Value::as_str)
                .map(str::to_string);
            let sender = TRANSCRIBES
                .lock()
                .unwrap()
                .as_mut()
                .and_then(|map| map.remove(&request_id));
            if let Some(sender) = sender {
                let _ = sender.send(match error {
                    Some(error) => Err(error),
                    None => Ok(text),
                });
            }
        }
        "voice.error" => {
            let message = text_field(&data, "message");
            tracing::error!("语音前端报错: {message}");
            notify(
                state,
                t("GQY voice stopped", "语音服务已停止"),
                &clip(&message, 120),
            );
        }
        other => tracing::debug!("忽略语音前端事件 {other}"),
    }
}

fn notify(state: &DaemonState, title: &str, body: &str) {
    let enabled = state.manager.lock().unwrap().config.notifications.enabled;
    if !enabled {
        return;
    }
    if !cfg!(target_os = "linux") {
        crate::notify::notify(title, body);
        return;
    }
    let sender = NOTIFY_QUEUE
        .lock()
        .unwrap()
        .get_or_insert_with(|| {
            let (tx, rx) = std::sync::mpsc::channel::<(String, String)>();
            std::thread::Builder::new()
                .name("gqy-voice-notify".into())
                .spawn(move || {
                    let mut last: Option<u32> = None;
                    for (title, body) in rx {
                        last = crate::notify::notify_replacing(&title, &body, last);
                    }
                })
                .ok();
            tx
        })
        .clone();
    if sender.send((title.to_string(), body.to_string())).is_err() {
        crate::notify::notify(title, body);
    }
}

/// 通知正文用的摘要:去掉 Markdown 记号、折叠空白、截到 `max` 字。
pub(crate) fn clip(text: &str, max: usize) -> String {
    let mut cleaned = String::with_capacity(text.len());
    let mut in_fence = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let stripped = trimmed
            .trim_start_matches(|ch: char| ch == '#' || ch == '>' || ch == '-' || ch == '*')
            .trim();
        if stripped.is_empty() {
            continue;
        }
        if !cleaned.is_empty() {
            cleaned.push(' ');
        }
        cleaned.push_str(&stripped.replace("**", "").replace('`', ""));
    }
    let chars: Vec<char> = cleaned.chars().collect();
    if chars.len() <= max {
        cleaned
    } else {
        let mut out: String = chars[..max.saturating_sub(1)].iter().collect();
        out.push('…');
        out
    }
}

/// 播报文本切一刀:先播的第一句 + 剩下的。剩下的为空表示不值得切。
///
/// 只切一刀,不逐句切碎:每段都是一次独立合成,接缝多了句子之间的语气就断得
/// 明显。第一句足够短就能把首声延迟压下来,后面整段合成保住连贯。
///
/// 太短的开头(「好的。」)不单独成段——为一个词多跑一次请求、多一个接缝,不
/// 划算;长到没有标点也不硬切,宁可整段合成。
fn split_first_sentence(text: &str) -> (&str, &str) {
    /// 第一句至少这么多字才值得单独合成。
    const MIN_CHARS: usize = 6;
    /// 这么多字还没见到句末标点就别切了。
    const MAX_CHARS: usize = 60;
    /// 尾巴短于这个就并回去。
    const TAIL_MIN_CHARS: usize = 4;

    let mut chars = text.char_indices().peekable();
    let mut count = 0usize;
    let mut cut = None;
    while let Some((index, ch)) = chars.next() {
        count += 1;
        if count > MAX_CHARS {
            return (text, "");
        }
        let ends_sentence = matches!(ch, '。' | '！' | '？' | '；' | '\n')
            // 英文标点要后面跟空白才算句末,否则 "3.5" 会被切开。
            || (matches!(ch, '.' | '!' | '?' | ';')
                && chars.peek().is_none_or(|(_, next)| next.is_whitespace()));
        if ends_sentence && count >= MIN_CHARS {
            cut = Some(index + ch.len_utf8());
            break;
        }
    }
    let Some(cut) = cut else {
        return (text, "");
    };
    let tail = text[cut..].trim_start();
    if tail.chars().count() < TAIL_MIN_CHARS {
        return (text, "");
    }
    (text[..cut].trim_end(), tail)
}

/// 语音回合落在**终端那条 REPL 会话**上:语音和终端是同一个顾清影,不是两
/// 个摊子。
///
/// 会话指针存在库里,终端关掉它也还在——所以没开终端时语音照样往这条会话里
/// 说,下次 `gqy` 进来 `ensure_repl_session` 拿到的是同一条,刚才用嘴聊的全
/// 在历史里接着聊。09-16 之前这里开的是一条专属 lane(id 记在 state 目录),
/// 于是终端里说过的话语音问不到,只能等记忆整理完靠联想撞运气。
///
/// 固定走 normal 人格车道,不跟随终端当时在 dev 还是 normal:人不在终端前的
/// 时候根本不知道当时是哪条,投错了很难受;随口问的东西也不该跑进 dev。
fn resolve_voice_session(state: &DaemonState) -> Result<String> {
    let persona = state.manager.lock().unwrap().config.active_persona_scope();
    state.state_store.ensure_repl_session(&persona)
}

/// 起这一轮并把回复文本收回来。
///
/// 不走自己的 IPC 口回环提交:那条路上 `job_wake` 写死 false、`display_content`
/// 等于 `content`。这里照 `goal_driver` 的做法直接进 actor,于是
/// ① `job_wake=true` 让 REPL 客户端发现并挂上实时渲染——终端开着的时候,语音
///    这一轮和手打的那一轮长得一模一样;② 原话与协议包裹能分开。
///
/// 终端没开也照跑:没人渲染而已,回复照样落进这条会话,TTS 照样念。
async fn collect_voice_reply(
    state: &DaemonState,
    session_id: &str,
    content: String,
    display_content: String,
) -> Result<(String, &'static str)> {
    use tokio::sync::broadcast::error::RecvError;

    let run_id = crate::runtime::random_id("run", 18);
    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    {
        let mut manager = state.manager.lock().unwrap();
        if manager.admin_blocks_session(session_id) {
            anyhow::bail!(crate::ipc::ADMIN_BUSY_MESSAGE);
        }
        manager.active_runs.insert(
            run_id.clone(),
            crate::runtime::RunInfo {
                session_id: session_id.into(),
                mode: crate::agent::AgentMode::Normal,
                audience: crate::config::PromptAudience::Owner,
                cancel: cancel_tx,
                turn_id: None,
                queue_target: None,
                supersede: std::sync::Arc::new(crate::agent::TurnSupersedeSignal::default()),
                platform_followup: None,
                operation: crate::runtime::RunOperation::Create,
                // 复用 job_wake 这条可见性通道(goal 续轮同款):REPL 客户端靠
                // 它发现 daemon 自己发起的回合。不开的话,语音这一轮在终端里
                // 是完全看不见的。
                job_wake: true,
                turn_origin: crate::tools::workspace::TurnOrigin::Human,
                job_wake_label: Some(crate::i18n::text("voice", "语音").to_string()),
            },
        );
    }
    *ACTIVE_RUN.lock().unwrap() = Some(run_id.clone());

    let after = state.events.latest_id();
    let mut subscription = state.events.subscribe_after(after);
    if state
        .actor_tx
        .send(crate::runtime::ActorCommand::StartTurn {
            run_id: run_id.clone(),
            session_id: session_id.into(),
            content,
            display_content,
            attachment_run_id: None,
            mode: crate::agent::AgentMode::Normal,
            images: Vec::new(),
            cwd: None,
            origin_tty: None,
            audience: crate::config::PromptAudience::Owner,
            profile: None,
            overrides: None,
            cancel: cancel_rx,
            turn_origin: Box::new(crate::tools::workspace::TurnOrigin::Human),
        })
        .is_err()
    {
        crate::runtime::finish_run(&state.manager, &run_id, None);
        anyhow::bail!("GQY core worker is unavailable");
    }

    let mut reply = String::new();
    let mut last_id = after;
    loop {
        let record = match subscription.pending.pop_front() {
            Some(record) => record,
            None => match subscription.receiver.recv().await {
                Ok(record) => record,
                Err(RecvError::Lagged(_)) => {
                    subscription.pending = state.events.replay_after(last_id);
                    continue;
                }
                Err(RecvError::Closed) => return Ok((reply, "disconnected")),
            },
        };
        if record.kind == "resync_required" {
            // 别的会话刷屏把本轮事件挤出了共享缓冲(ipc_server 同款处理):回合
            // 还在跑就续流,已经结束就按完成收尾——不谎报取消,否则那边刚说完
            // 这边就把播报吞了。
            last_id = serde_json::from_str::<Value>(&record.data)
                .ok()
                .and_then(|data| data.get("latest_event_id").and_then(Value::as_u64))
                .unwrap_or(last_id);
            let still_running = state
                .manager
                .lock()
                .unwrap()
                .active_runs
                .contains_key(&run_id);
            if still_running {
                continue;
            }
            return Ok((reply, "completed"));
        }
        last_id = record.id;
        let Ok(data) = serde_json::from_str::<Value>(&record.data) else {
            continue;
        };
        if data.get("run_id").and_then(Value::as_str) != Some(run_id.as_str()) {
            continue;
        }
        match record.kind.as_str() {
            "assistant.delta" => {
                if let Some(delta) = data.get("delta").and_then(Value::as_str) {
                    reply.push_str(delta);
                }
            }
            "run.completed" => return Ok((reply, "completed")),
            "run.cancelled" => return Ok((reply, "cancelled")),
            "run.failed" => {
                let message = data
                    .get("error")
                    .or_else(|| data.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error");
                anyhow::bail!("{message}");
            }
            _ => {}
        }
    }
}

/// 放开 hold——仅当 `gen` 仍是最新一代。见 [`HOLD_GEN`]。
fn release_hold(generation: u64) {
    if HOLD_GEN.load(Ordering::Relaxed) == generation {
        send_signal("voice.hold", json!({ "on": false }));
    }
}

/// 打断进行中的语音回合(有就取消,没有无操作)。
fn cancel_active_run(state: &DaemonState) {
    let Some(run_id) = ACTIVE_RUN.lock().unwrap().take() else {
        return;
    };
    let socket = state.paths.ipc_socket();
    tokio::spawn(async move {
        let outcome = async {
            let mut stream = crate::ipc::connect(&socket).await?;
            crate::ipc::send(
                &mut stream,
                &crate::ipc::Request::new(crate::ipc::Command::Cancel { run_id }),
            )
            .await?;
            let _ = crate::ipc::receive::<crate::ipc::Frame>(&mut stream).await;
            anyhow::Ok(())
        }
        .await;
        if let Err(error) = outcome {
            tracing::debug!("取消语音回合失败: {error:#}");
        }
    });
}

/// 一轮语音回合:经自己的 IPC 口提交(复用全部校验与回合机制),读事件流
/// 攒回复文本,完成后通知摘要 + done 提示音。
async fn run_voice_turn(state: &DaemonState, content: String) -> Result<()> {
    use crate::i18n::text as t;
    let session_id = resolve_voice_session(state)?;
    let (reply_chars, tts) = {
        let manager = state.manager.lock().unwrap();
        (
            manager.config.voice.notify_reply_chars,
            manager.config.voice.tts.clone(),
        )
    };
    // 语音协议(agent::prompt::VOICE_PROTOCOL):用户消息带这个包裹,模型才会
    // 在回复末尾给 <speak> 口语版。包裹只喂模型,`display_content` 给的是原
    // 话——这一轮现在在终端里看得见了,不该让用户读到一串尖括号。
    let display_content = content.clone();
    let content = format!("<voice_input>{content}</voice_input>");
    let window_gen = VOICE_WINDOW_GEN.load(Ordering::Relaxed);
    let hold_gen = HOLD_GEN.fetch_add(1, Ordering::Relaxed) + 1;
    send_signal("voice.hold", json!({ "on": true }));
    let outcome = collect_voice_reply(state, &session_id, content, display_content).await;
    // 只有当前仍是这一轮时才清 ACTIVE_RUN(被打断时新一轮可能已经接管)。
    let finished = outcome.as_ref().ok().map(|(_, how)| *how);
    if finished.is_some() || outcome.is_err() {
        let mut active = ACTIVE_RUN.lock().unwrap();
        if active.is_some() {
            *active = None;
        }
    }
    // hold 要压到合成完才放:追问窗口从"她回复完"起算,合成那一到三秒不该
    // 吃掉窗口;播报期间前端不喂帧,窗口同样不走,所以实际起算点是播完。
    let (reply, how) = match outcome {
        Ok(pair) => pair,
        Err(error) => {
            release_hold(hold_gen);
            return Err(error);
        }
    };
    let window_closed = || VOICE_WINDOW_GEN.load(Ordering::Relaxed) != window_gen;
    match how {
        "completed" if window_closed() => {
            // 回合跑完前窗口已被关(快捷键再按 / 没事了 / 超时):不念不弹。
            release_hold(hold_gen);
            tracing::debug!("语音回合完成时窗口已关,跳过播报");
        }
        "completed" => {
            // 通知正文与播报用同一份口语版:有 <speak> 用 <speak>,没有就清洗正文。
            let spoken = crate::web::voice_tts::spoken_text(&reply, &tts);
            let summary = clip(&spoken, reply_chars);
            let spoken = spoken.trim().to_string();
            if !tts.is_active() || spoken.is_empty() {
                release_hold(hold_gen);
                if window_closed() {
                    tracing::debug!("窗口已关,跳过提示音");
                    return Ok(());
                }
                notify(state, "顾清影", &summary);
                send_signal("voice.cue", json!({ "name": "done" }));
                return Ok(());
            }
            // 分句流水线(09-16):整段合成要一到三秒,这期间是全静默的干等。
            // 切一刀,第一句短、合得快,先出声;剩下的在它播的时候合,接上排队
            // 播出。首声延迟从「整段合成」降到「第一句合成」。
            let (head, tail) = split_first_sentence(&spoken);
            // 先合成再通知:通知若先弹,用户看到文字却要等好几秒才听到声音
            // (09-05)。合成好了通知与播放同一瞬间发出。
            let first = match synthesize_to_cache(state, &tts, head).await {
                Ok(path) => Some(path),
                Err(error) => {
                    tracing::warn!("播报失败: {error:#}");
                    None
                }
            };
            release_hold(hold_gen);
            if window_closed() {
                // 合成这一两秒里被关掉了:音频作废。
                if let Some(path) = first {
                    let _ = std::fs::remove_file(path);
                }
                tracing::debug!("合成期间窗口已关,丢弃播报");
                return Ok(());
            }
            notify(state, "顾清影", &summary);
            let Some(first) = first else {
                send_signal("voice.cue", json!({ "name": "done" }));
                return Ok(());
            };
            send_signal(
                "voice.play",
                json!({ "wav_path": first.display().to_string(), "more": !tail.is_empty() }),
            );
            if tail.is_empty() {
                return Ok(());
            }
            // 后半段在第一句播着的时候合。合不出来也不用补救:前端等不到下一
            // 段会在宽限期后自己收状态,掐掉正在播的第一句反而更糟。
            match synthesize_to_cache(state, &tts, tail).await {
                Ok(path) if window_closed() => {
                    let _ = std::fs::remove_file(path);
                    tracing::debug!("后半段合成期间窗口已关,丢弃");
                }
                Ok(path) => send_signal(
                    "voice.play",
                    json!({ "wav_path": path.display().to_string(), "more": false }),
                ),
                Err(error) => tracing::warn!("后半段播报合成失败: {error:#}"),
            }
        }
        "cancelled" => release_hold(hold_gen),
        _ => {
            release_hold(hold_gen);
            notify(
                state,
                t("GQY voice", "顾清影 语音"),
                t(
                    "connection to the daemon dropped mid-turn",
                    "回合中途与 daemon 断开",
                ),
            );
        }
    }
    Ok(())
}

/// 按当前 TTS 配置把文本变成语音交给前端播:daemon 调播报供应商合成 wav 落到
/// cache,再 `voice.play` 让前端播。
pub(crate) async fn speak(state: &DaemonState, text: &str) -> Result<()> {
    speak_with(state, text, None).await
}

/// 同 [`speak`],`override_tts` 为 Some 时用这份配置代替 daemon 当前配置
/// (设置界面试听未保存的音色/语速)。
pub(crate) async fn speak_with(
    state: &DaemonState,
    text: &str,
    override_tts: Option<crate::config::VoiceTtsConfig>,
) -> Result<()> {
    let tts =
        override_tts.unwrap_or_else(|| state.manager.lock().unwrap().config.voice.tts.clone());
    let text = text.trim();
    if text.is_empty() {
        return Ok(());
    }
    if !tts.is_active() {
        anyhow::bail!("回复播报未激活(语音功能 → 文本转语音开关 + 播报供应商填 key)");
    }
    let path = synthesize_to_cache(state, &tts, text).await?;
    send_signal(
        "voice.play",
        json!({ "wav_path": path.display().to_string() }),
    );
    Ok(())
}

/// 合成到 cache/voice/<id>.wav,返回路径。
async fn synthesize_to_cache(
    state: &DaemonState,
    tts: &crate::config::VoiceTtsConfig,
    text: &str,
) -> Result<PathBuf> {
    let wav = crate::web::voice_tts::synthesize(tts, text).await?;
    let dir = state.paths.cache_dir.join("voice");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.wav", crate::runtime::random_id("tts", 12)));
    std::fs::write(&path, wav)?;
    Ok(path)
}

/// 语音识别当下能不能用:语音唤醒开着(识别模型随它加载)且前端已接上。
/// 入站 QQ 语音转写用它决定"转"还是"静默留占位",不会去等前端拉起。
pub(crate) fn stt_available(state: &DaemonState) -> bool {
    let enabled = state.manager.lock().unwrap().config.voice.enabled;
    enabled && ATTACH.lock().unwrap().is_some()
}

/// 播报是否可用(daemon 内、开关开着、供应商激活)。平台工具注册用。
pub(crate) fn tts_available() -> bool {
    DAEMON_STATE
        .get()
        .is_some_and(|state| state.manager.lock().unwrap().config.voice.tts.is_active())
}

/// `send_voice_message` 工具用:把文本合成成 wav 文件(QQ 语音消息),不播。
pub(crate) async fn synthesize_for_platform(text: &str) -> Result<PathBuf> {
    let state = DAEMON_STATE
        .get()
        .context("send_voice_message 只能在 daemon 里用")?;
    let tts = state.manager.lock().unwrap().config.voice.tts.clone();
    if !tts.is_active() {
        anyhow::bail!("文本转语音未开启或未激活供应商(设置 → 语音功能)");
    }
    let text = text.trim();
    anyhow::ensure!(!text.is_empty(), "text is empty");
    let spoken = crate::web::voice_tts::sanitize_for_speech(text);
    let spoken = if spoken.trim().is_empty() {
        text.to_string()
    } else {
        spoken
    };
    synthesize_to_cache(state, &tts, &spoken).await
}

/// daemon 的状态句柄,给 `speak` 工具这类没有 state 参数的调用点用。
static DAEMON_STATE: std::sync::OnceLock<DaemonState> = std::sync::OnceLock::new();

pub(crate) fn install_state(state: &DaemonState) {
    let _ = DAEMON_STATE.set(state.clone());
}

/// daemon 状态句柄(非 daemon 进程里为 None)。
pub(crate) fn daemon_state() -> Option<&'static DaemonState> {
    DAEMON_STATE.get()
}

/// `speak` 工具入口:模型主动说话。前端未就绪时拉起并等它。
pub(crate) async fn speak_from_tool(text: &str) -> Result<()> {
    let state = DAEMON_STATE
        .get()
        .context("speak 只能在 daemon 里用(当前不是 daemon 进程)")?;
    if let Err(message) = wait_attached(state, 20_000).await {
        anyhow::bail!("{message}");
    }
    speak(state, text).await
}

/// VoiceSpeak 处理:`gqy voice say` / 设置页试听。
pub(crate) async fn handle_voice_speak(
    state: &DaemonState,
    stream: &mut tokio::net::UnixStream,
    text: String,
    override_tts: Option<crate::config::VoiceTtsConfig>,
) -> Result<()> {
    if let Err(message) = wait_attached(state, 20_000).await {
        crate::ipc::send(stream, &crate::ipc::Frame::error(message)).await?;
        return Ok(());
    }
    match speak_with(state, &text, override_tts).await {
        Ok(()) => crate::ipc::send(stream, &crate::ipc::Frame::Ack).await?,
        Err(error) => {
            crate::ipc::send(stream, &crate::ipc::Frame::error(format!("{error:#}"))).await?
        }
    }
    Ok(())
}

/// VoiceReset 处理:掐掉在跑的语音回合,并清掉 09-16 之前那条专属语音会话的
/// 残留。
///
/// **不碰当前会话**——语音现在跟终端共用一条会话(见 [`resolve_voice_session`]),
/// 再照老样子按标记删会话,删掉的就是用户自己的终端对话了。所以只删 kind 确实
/// 是 `voice` 的那条遗留会话,顺手把标记文件清掉。
pub(crate) async fn handle_voice_reset(
    state: &DaemonState,
    stream: &mut tokio::net::UnixStream,
) -> Result<()> {
    cancel_active_run(state);
    let marker = state.paths.state_dir.join(SESSION_ID_FILE);
    if let Ok(saved) = std::fs::read_to_string(&marker) {
        let saved = saved.trim();
        let legacy = !saved.is_empty()
            && state
                .state_store
                .session_record(saved)
                .ok()
                .flatten()
                .is_some_and(|record| record.kind == crate::state::VOICE_SESSION_KIND);
        if legacy {
            if let Err(error) = state.state_store.delete_session(saved) {
                tracing::warn!("删除遗留语音会话 {saved} 失败: {error:#}");
            }
        }
    }
    let _ = std::fs::remove_file(&marker);
    crate::ipc::send(stream, &crate::ipc::Frame::Ack).await?;
    Ok(())
}

/// 等 worker 的信令连接就绪(冷启动要加载模型,给足时间)。
async fn wait_attached(state: &DaemonState, timeout_ms: u64) -> Result<(), &'static str> {
    let wanted = worker_wanted(&state.manager.lock().unwrap().config.voice);
    if !wanted {
        return Err("语音唤醒和文本转语音都没开(设置 → 语音功能)");
    }
    if locate_binary().is_none() {
        return Err("找不到 gqy-voice 可执行文件,请安装语音组件");
    }
    ensure_worker(state);
    let mut waited = 0u64;
    while !worker_running() {
        if waited >= timeout_ms {
            return Err("语音前端未就绪(模型缺失或启动失败,见 voice-worker.log)");
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        waited += 250;
    }
    Ok(())
}

/// 认领听写流(同一时刻只能有一个认领者)。`external` 为 true 时音频由
/// 认领者经 [`push_audio`] 推入(浏览器麦克风),否则前端用本机麦克风。
/// 返回识别文本的接收端;认领者用完必须调 [`release_dictation`]。
pub(crate) async fn claim_dictation(
    state: &DaemonState,
    external: bool,
) -> std::result::Result<UnboundedReceiver<DictationRelay>, &'static str> {
    if !state.manager.lock().unwrap().config.voice.enabled {
        return Err("语音唤醒未开启(设置 → 语音功能),听写需要麦克风");
    }
    wait_attached(state, 20_000).await?;
    let (tx, rx) = unbounded_channel();
    {
        let mut slot = DICTATION.lock().unwrap();
        if slot.is_some() {
            return Err("已有一个听写会话在进行");
        }
        *slot = Some(tx);
    }
    cancel_active_run(state);
    send_signal(
        "voice.dictation",
        json!({ "on": true, "source": if external { "stream" } else { "mic" } }),
    );
    Ok(rx)
}

/// 释放听写认领:前端关听写窗、恢复唤醒。重复调用无害。
pub(crate) fn release_dictation() {
    DICTATION.lock().unwrap().take();
    send_signal("voice.dictation", json!({ "on": false }));
}

/// 外部听写的音频块(16kHz 单声道 PCM16 LE)转给前端。
pub(crate) fn push_audio(pcm16: &[u8]) {
    use base64::Engine;
    if pcm16.len() < 2 {
        return;
    }
    send_signal(
        "voice.audio",
        json!({ "pcm16": base64::engine::general_purpose::STANDARD.encode(pcm16) }),
    );
}

/// HTTP 面用的就绪等待(错误文本直接给前端)。
pub(crate) async fn wait_attached_public(state: &DaemonState) -> std::result::Result<(), String> {
    wait_attached(state, 20_000).await.map_err(str::to_string)
}

/// StartDictation 处理:认领听写流,转写 Event 帧流回客户端,断开即释放。
pub(crate) async fn handle_start_dictation(
    state: &DaemonState,
    stream: &mut tokio::net::UnixStream,
) -> Result<()> {
    let mut rx = match claim_dictation(state, false).await {
        Ok(rx) => rx,
        Err(message) => {
            crate::ipc::send(stream, &crate::ipc::Frame::error(message)).await?;
            return Ok(());
        }
    };
    crate::ipc::send(stream, &crate::ipc::Frame::Ack).await?;
    loop {
        tokio::select! {
            event = rx.recv() => match event {
                Some(DictationRelay::Utterance(text)) => {
                    crate::ipc::send(stream, &crate::ipc::Frame::Event {
                        id: 0,
                        kind: "voice.dictation".to_string(),
                        data: json!({ "text": text }),
                    }).await?;
                }
                Some(DictationRelay::Ended) | None => {
                    let _ = crate::ipc::send(stream, &crate::ipc::Frame::Event {
                        id: 0,
                        kind: "voice.dictation_ended".to_string(),
                        data: json!({}),
                    }).await;
                    break;
                }
            },
            // 客户端断开(Ctrl+C/关终端/Esc):释放认领。
            frame = crate::ipc::receive::<crate::ipc::Frame>(stream) => {
                let _ = frame;
                break;
            }
        }
    }
    release_dictation();
    Ok(())
}

/// VoiceListen 处理:快捷键呼叫。前端进入等待指令,后续与唤醒命中一样。
pub(crate) async fn handle_voice_listen(
    state: &DaemonState,
    stream: &mut tokio::net::UnixStream,
) -> Result<()> {
    if !state.manager.lock().unwrap().config.voice.enabled {
        crate::ipc::send(
            stream,
            &crate::ipc::Frame::error("语音唤醒未开启(设置 → 语音功能)"),
        )
        .await?;
        return Ok(());
    }
    if let Err(message) = wait_attached(state, 20_000).await {
        crate::ipc::send(stream, &crate::ipc::Frame::error(message)).await?;
        return Ok(());
    }
    if DICTATION.lock().unwrap().is_some() {
        crate::ipc::send(stream, &crate::ipc::Frame::error("听写进行中,先结束听写")).await?;
        return Ok(());
    }
    cancel_active_run(state);
    send_signal("voice.listen", json!({}));
    crate::ipc::send(stream, &crate::ipc::Frame::Ack).await?;
    Ok(())
}

/// VoiceStatus 处理。
pub(crate) async fn handle_voice_status(
    state: &DaemonState,
    stream: &mut tokio::net::UnixStream,
) -> Result<()> {
    crate::ipc::send(
        stream,
        &crate::ipc::Frame::Event {
            id: 0,
            kind: "voice.status".to_string(),
            data: status(state),
        },
    )
    .await?;
    Ok(())
}

fn fail_all_transcribes(reason: &str) {
    let pending = TRANSCRIBES.lock().unwrap().take();
    if let Some(map) = pending {
        for (_, sender) in map {
            let _ = sender.send(Err(reason.to_string()));
        }
    }
}

/// 整段录音(16k 单声道 PCM WAV)→ 文本。worker 必须在跑。WebUI 麦克风
/// 已改走流式听写,这条留给外部脚本/集成。
pub(crate) async fn transcribe_wav(state: &DaemonState, wav: &[u8]) -> Result<String> {
    if let Err(message) = wait_attached(state, 20_000).await {
        anyhow::bail!("{message}");
    }
    let dir = state.paths.cache_dir.join("voice");
    std::fs::create_dir_all(&dir)?;
    let request_id = crate::runtime::random_id("stt", 12);
    let path = dir.join(format!("{request_id}.wav"));
    std::fs::write(&path, wav)?;
    let (tx, rx) = oneshot::channel();
    TRANSCRIBES
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .insert(request_id.clone(), tx);
    send_signal(
        "voice.transcribe",
        json!({ "request_id": request_id, "wav_path": path.display().to_string() }),
    );
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(60), rx).await;
    let _ = std::fs::remove_file(&path);
    match outcome {
        Ok(Ok(Ok(text))) => Ok(text),
        Ok(Ok(Err(message))) => anyhow::bail!("{message}"),
        Ok(Err(_)) => anyhow::bail!("语音前端未应答"),
        Err(_) => {
            if let Some(map) = TRANSCRIBES.lock().unwrap().as_mut() {
                map.remove(&request_id);
            }
            anyhow::bail!("转写超时")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{clip, split_first_sentence};

    #[test]
    fn clip_strips_markdown_and_truncates() {
        let text = "# 标题\n\n**重点**:今天 `晴`。\n```rust\nlet x = 1;\n```\n- 第二行";
        assert_eq!(clip(text, 100), "标题 重点:今天 晴。 第二行");
        assert_eq!(clip("一二三四五六", 4), "一二三…");
    }

    #[test]
    fn first_sentence_is_split_off_for_early_playback() {
        assert_eq!(
            split_first_sentence("日志我看过了。问题出在重连那段,退避没有上限。"),
            ("日志我看过了。", "问题出在重连那段,退避没有上限。")
        );
        assert_eq!(
            split_first_sentence("Checked the log. The backoff has no ceiling."),
            ("Checked the log.", "The backoff has no ceiling.")
        );
    }

    #[test]
    fn short_lead_ins_are_not_split_off() {
        // 「好的。」单独合成一段不划算:多一次请求、多一个接缝。
        let text = "好的。我这就去看看那个文件到底写了什么。";
        assert_eq!(split_first_sentence(text), (text, ""));
    }

    #[test]
    fn a_lone_sentence_stays_whole() {
        let text = "这一句里没有句号所以整段合成";
        assert_eq!(split_first_sentence(text), (text, ""));
        // 尾巴太短也并回去。
        let text = "我看完日志了,没发现问题。好";
        assert_eq!(split_first_sentence(text), (text, ""));
    }

    #[test]
    fn decimals_are_not_sentence_boundaries() {
        let text = "版本升到 3.5 之后重连就正常了,昨天那批告警也没再出现。";
        let (head, tail) = split_first_sentence(text);
        assert_eq!(head, text);
        assert_eq!(tail, "");
    }

    #[test]
    fn a_long_run_on_is_left_whole() {
        // 六十字还没见到标点:宁可整段合成,也不硬切出怪断句。
        let text = "这段话很长但是一个标点都没有所以不应该被切开因为硬切出来的断句听起来会很奇怪还不如整段合成来得自然一些就这样吧";
        assert_eq!(split_first_sentence(text), (text, ""));
    }
}
