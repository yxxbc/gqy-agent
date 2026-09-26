//! Cline CLI 中转协议(`protocol = "cline"`)。
//!
//! 传输层是本机 `cline` 子进程的 NDJSON 事件流(`cline --json "<prompt>"`):
//! CLI 用用户既有的 `~/.cline` 登录态,顾清影 不经手任何凭据;工具循环的所有权
//! 在 cline 侧,顾清影 的工具经 `gqy mcp-serve` 桥挂进去(与 claude 线同构),
//! 所以这条线对 顾清影 的回合循环呈现为「一次请求、纯文本(+思考)回来、永远
//! 没有 tool_calls」。
//!
//! 与另三条线不同的两样:
//! ①提示词走**命令行位置参数**(这一版 `--json` 不收 stdin,09-26 实机探针),
//!   Linux 单个参数上限 128KiB ⇒ 载荷带字节预算,超了从最老的历史丢起;
//! ②CLI 不在事件流里报会话 id(只有 agentId/taskId):续传靠扫描
//!   `~/.cline/data/sessions/` 里会话文件的 `prompt` 与 mtime 核对(见
//!   [`session`]),核对不过就忘掉这条链、下一轮全量重放——只损失效率,
//!   不损失正确性。

mod session;
mod stream;

use crate::llm::openai_compatible::cli_relay::{
    self, payload, RelayOutcome, ResumePlan, ToolScopes,
};
use crate::llm::openai_compatible::*;

/// 客户端构造期解析好的运行时参数,端点间共享。
pub(in crate::llm::openai_compatible) struct ClineRuntime {
    pub(in crate::llm::openai_compatible) binary: PathBuf,
    /// `-P` 显式供应商 id(空 = 让 CLI 走自己默认的那家)。
    pub(in crate::llm::openai_compatible) provider: String,
    /// cline 原生工具的模式作用域:off/dev/normal/all。
    pub(in crate::llm::openai_compatible) native_tools: String,
    /// 顾清影 工具经 MCP 桥挂给 cline 的模式作用域:off/dev/normal/all。
    pub(in crate::llm::openai_compatible) gqy_tools: String,
    pub(in crate::llm::openai_compatible) idle_timeout: Duration,
    /// 每轮桥配置文件落盘目录(`<state>/relay/cline`)。
    pub(in crate::llm::openai_compatible) relay_dir: PathBuf,
    /// CLI 的数据目录(`CLINE_DATA_DIR` 或 `~/.cline/data`);续传核对的原料。
    pub(in crate::llm::openai_compatible) data_dir: PathBuf,
}

impl ClineRuntime {
    pub(in crate::llm::openai_compatible) fn from_config(config: &AppConfig) -> Self {
        let plugin = &config.plugins.cline;
        let binary = if plugin.binary.trim().is_empty() {
            PathBuf::from("cline")
        } else {
            PathBuf::from(plugin.binary.trim())
        };
        Self {
            binary,
            provider: plugin.provider.clone(),
            native_tools: plugin.native_tools.clone(),
            gqy_tools: plugin.gqy_tools.clone(),
            idle_timeout: Duration::from_secs(plugin.idle_timeout_seconds.max(30)),
            relay_dir: default_relay_dir(),
            data_dir: session::data_dir(),
        }
    }
}

/// `<state>/relay/cline`;拿不到 顾清影 路径时退到临时目录。
fn default_relay_dir() -> PathBuf {
    crate::paths::GqyPaths::new()
        .map(|paths| paths.state_dir.join("relay").join("cline"))
        .unwrap_or_else(|_| std::env::temp_dir().join("gqy-cline"))
}

/// 清空 顾清影 会话时的联动(cli_relay::forget_relay_sessions 调用):尽力删除
/// cline 侧的会话目录。删不到只是浪费磁盘,映射已经丢了。
pub(in crate::llm::openai_compatible) fn remove_session_files(id: &str) {
    session::remove_session(&session::data_dir(), id);
}

/// 两套工具同开时从桥里剔除的 顾清影 工具。cline 原生有命令执行、文件读写、
/// 搜索与网页工具(=run_command/edit/glob/grep/web_search/web_fetch)和待办
/// 清单(=todowrite);read/subagent/job/alarm 的保留理由同 claude 线。
pub(in crate::llm::openai_compatible) const BRIDGE_DUPLICATE_TOOLS: &[&str] = &[
    "run_command",
    "web_search",
    "web_fetch",
    "glob",
    "grep",
    "todowrite",
    "edit",
];

/// 中转环境事实(声明式,不写指令;常量字节保证提示词哈希稳定)。
const RELAY_ENVIRONMENT_NOTE: &str = "\n\n<relay-environment>\nThis session runs inside GQY's relay. Each turn is a fresh cline process that exits when the turn ends. Background work started through the built-in tools dies with the process.\n</relay-environment>";

/// gqy 工具桥在场时的补充事实。
const RELAY_GQY_TOOLS_NOTE: &str = "\n<relay-environment-tools>\nThe MCP tools served by the GQY bridge live in the persistent GQY daemon and survive across turns. The subagent tool runs a background subagent that wakes a follow-up turn when it finishes. The job tool inspects or stops those. The alarm tool schedules timed reminders.\n</relay-environment-tools>";

/// 单条 argv 载荷的字节预算:Linux 单个参数上限 128KiB(MAX_ARG_STRLEN),
/// 取 96KiB 留余量。超了 [`payload::render_user_blocks`] 从最老的历史丢起,
/// 本轮的真实输入永远保留;尾巴自己就超预算时不动它,只留痕。
const ARG_BYTE_BUDGET: Option<usize> = Some(96 * 1024);

/// 实测发现 `--id` 没被兑现(目标会话没被本轮写过)时拉闸:本进程内这条
/// provider 不再记任何续传映射,全量重放到底——宁可多花 token,不给「静默
/// 丢历史」留活口。重启 daemon 后重试(CLI 升级/修好之后自动恢复)。
static RESUME_BROKEN: LazyLock<Mutex<HashSet<String>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

fn resume_is_broken(provider_id: &str) -> bool {
    RESUME_BROKEN
        .lock()
        .map(|set| set.contains(provider_id))
        .unwrap_or(false)
}

fn mark_resume_broken(provider_id: &str) {
    if let Ok(mut set) = RESUME_BROKEN.lock() {
        if set.insert(provider_id.to_string()) {
            tracing::warn!(
                provider = provider_id,
                "cline did not write to the resumed session; resume is off for this provider until the daemon restarts (full replay from now on)"
            );
        }
    }
}

/// 增量消息 → 一条 argv 位置参数:正文块依次拼接,媒体块降级成占位符
/// (cline 的提示词是纯文本,内联块进不去)。
fn render_prompt(delta: &[ChatMessage]) -> String {
    let blocks = payload::render_user_blocks(delta, ARG_BYTE_BUDGET);
    let mut text = String::new();
    for block in blocks {
        if !text.is_empty() {
            text.push_str("\n\n");
        }
        match block.get("text").and_then(Value::as_str) {
            Some(part) => text.push_str(part),
            None => text.push_str("[attachment omitted: the cline relay accepts text only]"),
        }
    }
    text
}

/// 本轮桥配置文件:`CLINE_MCP_SETTINGS_PATH` 指过去,CLI 本轮读到的 MCP 面
/// 就只有 `gqy` 一条——用户自己的 MCP 服务器不进中转(与 claude 线的
/// `--strict-mcp-config` 同义)。进程收口后删掉,成功失败都删。
struct TempMcpSettings {
    path: PathBuf,
}

impl Drop for TempMcpSettings {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn write_mcp_settings(
    runtime: &ClineRuntime,
    gqy_session: &str,
    exclude_duplicates: bool,
) -> Option<TempMcpSettings> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let exe = crate::paths::gqy_executable().ok()?;
    let origin = serde_json::to_string(&crate::tools::workspace::current_turn_origin()).ok()?;
    let mut env = serde_json::Map::new();
    env.insert("GQY_SESSION".into(), json!(gqy_session));
    env.insert("GQY_TURN_ORIGIN".into(), json!(origin));
    if exclude_duplicates {
        env.insert(
            "GQY_MCP_EXCLUDE".into(),
            json!(BRIDGE_DUPLICATE_TOOLS.join(",")),
        );
    }
    for (key, value) in cli_relay::bridge_env_passthrough() {
        env.insert(key, json!(value));
    }
    let settings = json!({
        "mcpServers": {
            "gqy": {
                "type": "stdio",
                "command": exe,
                "args": ["mcp-serve"],
                "env": env,
            }
        }
    });
    std::fs::create_dir_all(&runtime.relay_dir).ok()?;
    let path = runtime.relay_dir.join(format!(
        "mcp-{}-{}.json",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, settings.to_string()).ok()?;
    Some(TempMcpSettings { path })
}

/// `cline --json "<prompt>"` 的参数表;tests 直接断言这份。
fn cline_args(
    runtime: &ClineRuntime,
    model: &str,
    workdir: &std::path::Path,
    system_prompt: &str,
    thinking: Option<&str>,
    resume: Option<&str>,
    prompt: &str,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "--json".into(),
        "--auto-approve".into(),
        "true".into(),
        "-c".into(),
        workdir.display().to_string(),
    ];
    if !runtime.provider.trim().is_empty() {
        args.push("-P".into());
        args.push(runtime.provider.trim().to_string());
    }
    if !model.trim().is_empty() {
        args.push("-m".into());
        args.push(model.to_string());
    }
    // 思考档:顾清影 的选择映射到 CLI 的 --thinking(none/low/medium/high/xhigh)。
    if let Some(thinking) = thinking.filter(|level| !level.trim().is_empty()) {
        args.push("--thinking".into());
        args.push(thinking.to_string());
    }
    // 人格整体替换 CLI 的内置系统提示词,别让 cline 的身份混进来。
    if !system_prompt.trim().is_empty() {
        args.push("-s".into());
        args.push(system_prompt.to_string());
    }
    if let Some(resume) = resume.filter(|id| !id.trim().is_empty()) {
        args.push("--id".into());
        args.push(resume.to_string());
    }
    // 提示词是位置参数,必须最后。
    args.push(prompt.to_string());
    args
}

impl OpenAiCompatibleClient {
    pub(crate) async fn chat_cline_stream<F>(
        &self,
        messages: Vec<ChatMessage>,
        _tools: Vec<ToolDefinition>,
        request_id: &str,
        on_chunk: &mut F,
    ) -> Result<ChatResult>
    where
        F: FnMut(ChatStreamChunk) -> Result<()>,
    {
        let runtime = self
            .cline
            .clone()
            .context("cline runtime was not initialized for this client")?;
        let model = self.provider.default_model.clone();
        let (system_prompt, conversation) = payload::split_system(messages);
        let workdir = crate::tools::workspace::effective_workdir();
        let gqy_session = crate::tools::workspace::try_session();
        let gqy_session = gqy_session.as_deref();
        let host_tools = cli_relay::host_tools_face(gqy_session);
        let scopes = cli_relay::tool_scopes(
            self.request_scope,
            &runtime.native_tools,
            &runtime.gqy_tools,
            self.claude_code_dev_mode,
        );
        let prompt = cli_relay::compose_prompt(
            &system_prompt,
            scopes,
            RELAY_ENVIRONMENT_NOTE,
            RELAY_GQY_TOOLS_NOTE,
        );
        let mut plan = ResumePlan::new(
            &self.provider.id,
            &model,
            &prompt,
            conversation,
            self.request_scope,
            gqy_session,
            host_tools,
        );
        // 目标会话不在了(被清理/换机)或 `--id` 已实证不可信:开跑之前就退化
        // 成全量重放——传进去只会先抛错白跑一轮。
        if let Some(id) = plan.resume_id() {
            if resume_is_broken(&self.provider.id)
                || !session::session_target_exists(&runtime.data_dir, id)
            {
                plan.resume_lost(
                    "cline",
                    request_id,
                    &anyhow::anyhow!("cline session {id} is no longer resumable"),
                );
            }
        }
        let mut outcome = self
            .cline_turn(
                &runtime,
                &model,
                &workdir,
                &prompt,
                scopes,
                gqy_session,
                &plan,
                request_id,
                on_chunk,
            )
            .await;
        if let Err(error) = &outcome {
            if plan.resume_id().is_some() && stream::resume_lost(error) {
                plan.resume_lost("cline", request_id, error);
                outcome = self
                    .cline_turn(
                        &runtime,
                        &model,
                        &workdir,
                        &prompt,
                        scopes,
                        gqy_session,
                        &plan,
                        request_id,
                        on_chunk,
                    )
                    .await;
            }
        }
        let outcome = outcome?;
        plan.record(&outcome);
        Ok(outcome.result)
    }

    #[allow(clippy::too_many_arguments)]
    async fn cline_turn<F>(
        &self,
        runtime: &ClineRuntime,
        model: &str,
        workdir: &std::path::Path,
        prompt: &str,
        scopes: ToolScopes,
        gqy_session: Option<&str>,
        plan: &ResumePlan,
        request_id: &str,
        on_chunk: &mut F,
    ) -> Result<RelayOutcome>
    where
        F: FnMut(ChatStreamChunk) -> Result<()>,
    {
        let payload_text = render_prompt(plan.delta());
        let resume_id = plan.resume_id().map(str::to_string);
        // 本轮窗口起点放宽两秒:粗粒度时间戳的文件系统也认得出"被本轮写过"。
        let started = std::time::SystemTime::now()
            .checked_sub(Duration::from_secs(2))
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let thinking =
            self.selected_reasoning_variant()
                .and_then(|(_, variant)| match variant.setting {
                    crate::models_cache::ReasoningSetting::Effort(effort) => Some(effort),
                    _ => None,
                });
        let args = cline_args(
            runtime,
            model,
            workdir,
            prompt,
            thinking.as_deref(),
            resume_id.as_deref(),
            &payload_text,
        );
        let mcp_settings = match (scopes.gqy_on, gqy_session) {
            (true, Some(session)) => write_mcp_settings(runtime, session, scopes.native_on),
            _ => None,
        };
        let env: Vec<(String, Option<String>)> = vec![(
            "CLINE_MCP_SETTINGS_PATH".to_string(),
            mcp_settings
                .as_ref()
                .map(|file| file.path.display().to_string()),
        )];
        crate::llm::request_log::record(
            &self.provider.id,
            model,
            "cline",
            self.request_scope,
            &runtime.binary.display().to_string(),
            // prompt 是续传核对的原料,录下来才能诊断"为什么没续上"。
            &json!({ "args": args, "prompt": payload_text, "conversation": plan.conversation() }),
        );
        let result =
            stream::run_cline_turn(runtime, workdir, &args, &env, request_id, on_chunk).await?;
        let session_id = match resume_id.as_deref() {
            Some(id) => {
                if session::touched_since(&runtime.data_dir, id, started, &payload_text) {
                    Some(id.to_string())
                } else {
                    mark_resume_broken(&self.provider.id);
                    None
                }
            }
            None if resume_is_broken(&self.provider.id) => None,
            None => {
                let discovered = session::discover(&runtime.data_dir, &payload_text, workdir);
                if discovered.is_none() {
                    tracing::info!(
                        provider = %self.provider.id,
                        sessions = %runtime.data_dir.display(),
                        "cline session could not be identified after the turn; the next turn will replay the full conversation"
                    );
                }
                discovered
            }
        };
        Ok(RelayOutcome { result, session_id })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime(provider: &str) -> ClineRuntime {
        ClineRuntime {
            binary: PathBuf::from("cline"),
            provider: provider.to_string(),
            native_tools: "all".to_string(),
            gqy_tools: "all".to_string(),
            idle_timeout: Duration::from_secs(300),
            relay_dir: std::env::temp_dir().join("gqy-cline-relay-test"),
            data_dir: std::env::temp_dir().join("gqy-cline-data-test"),
        }
    }

    fn has_pair(args: &[String], flag: &str, value: &str) -> bool {
        args.windows(2)
            .any(|pair| pair[0] == flag && pair[1] == value)
    }

    #[test]
    fn args_carry_flags_and_keep_the_prompt_last() {
        let args = cline_args(
            &runtime("cline-pass"),
            "anthropic/claude-sonnet-4.6",
            std::path::Path::new("/tmp/work"),
            "persona",
            Some("high"),
            Some("session_1"),
            "hello",
        );
        assert_eq!(args.first().map(String::as_str), Some("--json"));
        assert!(has_pair(&args, "--auto-approve", "true"));
        assert!(has_pair(&args, "-c", "/tmp/work"));
        assert!(has_pair(&args, "-P", "cline-pass"));
        assert!(has_pair(&args, "-m", "anthropic/claude-sonnet-4.6"));
        assert!(has_pair(&args, "--thinking", "high"));
        assert!(has_pair(&args, "-s", "persona"));
        assert!(has_pair(&args, "--id", "session_1"));
        // 位置参数必须最后:commander 把第一个非选项当提示词。
        assert_eq!(args.last().map(String::as_str), Some("hello"));
    }

    #[test]
    fn empty_optional_flags_are_omitted() {
        let args = cline_args(
            &runtime(""),
            "",
            std::path::Path::new("/tmp/work"),
            "",
            None,
            None,
            "hi",
        );
        assert!(!args
            .iter()
            .any(|arg| { matches!(arg.as_str(), "-P" | "-m" | "-s" | "--id" | "--thinking") }));
        assert_eq!(args.last().map(String::as_str), Some("hi"));
    }

    #[test]
    fn attachments_degrade_to_a_placeholder() {
        let delta = vec![ChatMessage::user_with_image(
            "look at this",
            "data:image/png;base64,AAAA",
        )];
        let text = render_prompt(&delta);
        assert!(text.contains("look at this"));
        assert!(text.contains("attachment omitted"));
        assert!(!text.contains("AAAA"));
    }

    #[test]
    fn full_replay_frames_the_history_block() {
        let delta = vec![
            ChatMessage::plain("user", "first question"),
            ChatMessage::assistant("first answer", None),
            ChatMessage::plain("user", "second question"),
        ];
        let text = render_prompt(&delta);
        assert!(text.contains("<conversation-history>"));
        assert!(text.contains("first question"));
        // 活跃尾巴(最后一条 user)在历史块之外。
        assert!(text.contains("second question"));
    }
}
