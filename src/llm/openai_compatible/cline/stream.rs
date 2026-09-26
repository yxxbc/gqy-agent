//! cline 子进程的 NDJSON 事件泵。
//!
//! `cline --json "<prompt>"` 每行一个 JSON(`@cline/shared` 的 `AgentEvent` 口径):
//! `{"type":"agent_event","event":{…}}` 里有 `content_start|update|end`
//! (text/reasoning/tool 三种 contentType)、`usage`、`done`、`error`、`notice`、
//! `iteration_*`;另有 `hook_event`(agent_start/tool_call/…)与 `team_event`,
//! 与中转无关只记日志。失败面:stdout 提前收场 + stderr 上一行
//! `{"type":"error","message":…}`(09-26 实机探针)。进程本身(拉起/看门狗/
//! stderr/击杀)在 [`cli_relay::process`]。

use crate::llm::openai_compatible::cli_relay::{
    hidden_remote_tool, process::RelayProcess, shape_remote_output,
};
use crate::llm::openai_compatible::cline::ClineRuntime;
use crate::llm::openai_compatible::*;
use serde_json::Value;

/// `--id` 目标在 cline 侧续不上的签名。CLI 没有公开的报错文案,按"找不到
/// 会话"这一类收口;误判的代价只是白跑一轮(调用方回退全量重放)。
pub(super) fn resume_lost(error: &anyhow::Error) -> bool {
    let text = format!("{error:#}").to_ascii_lowercase();
    [
        "session not found",
        "no such session",
        "unknown session",
        "session does not exist",
        "could not find session",
        "invalid session",
        "failed to resume",
    ]
    .iter()
    .any(|needle| text.contains(needle))
}

/// 把登录态失败的措辞翻成端点调度认识的分类(与另三条线同一套收口)。
fn classify_cline_failure(text: &str) -> Option<HttpStatusFailure> {
    let lower = text.to_ascii_lowercase();
    // 只认措辞,不认裸数字:错误文本里混着时间戳/token 数。
    const RATE_LIMIT: &[&str] = &[
        "rate limit",
        "too many requests",
        "usage limit",
        "quota",
        "out of credits",
    ];
    const AUTH: &[&str] = &[
        "unauthorized",
        "re-authenticate",
        "not logged in",
        "please log in",
        "authentication",
        "invalid api key",
    ];
    if RATE_LIMIT.iter().any(|needle| lower.contains(needle)) {
        return Some(HttpStatusFailure {
            status: 429,
            kind: HttpFailureKind::RateLimit,
        });
    }
    if AUTH.iter().any(|needle| lower.contains(needle)) {
        return Some(HttpStatusFailure {
            status: 401,
            kind: HttpFailureKind::Authentication,
        });
    }
    None
}

#[derive(Default)]
struct StreamState {
    content: String,
    content_emitted: usize,
    reasoning: String,
    reasoning_emitted: usize,
    /// 已发过 started 的工具 id → 展示名。
    started_tools: HashMap<String, String>,
    /// error 事件/非 JSON 杂音的正文,失败时并进报错。
    error_text: String,
    usage: Option<Usage>,
    failed: Option<String>,
    /// 收到 `done` 帧(CLI 认为本轮收口)。
    done: bool,
    finish_reason: Option<String>,
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_cline_turn<F>(
    runtime: &ClineRuntime,
    workdir: &std::path::Path,
    args: &[String],
    env: &[(String, Option<String>)],
    request_id: &str,
    on_chunk: &mut F,
) -> Result<ChatResult>
where
    F: FnMut(ChatStreamChunk) -> Result<()>,
{
    // stdin 只是要个"关掉的写端":这一版 `--json` 不收 stdin,提示词是位置参数。
    let mut process = RelayProcess::spawn(
        &runtime.binary,
        args,
        workdir,
        env,
        "",
        runtime.idle_timeout,
        "cline.stream",
        "cline",
        || {
            t(
                "Cline CLI not found; install it or set plugins.cline.binary",
                "找不到 Cline CLI;请安装它或配置 plugins.cline.binary",
            )
            .to_string()
        },
    )
    .await?;

    let mut state = StreamState::default();
    while let Some(line) = process.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(trimmed) {
            Ok(value) => value,
            Err(_) => {
                // cline 在 NDJSON 之外也会打人话(警告/提示),留作报错线索。
                tracing::debug!(
                    request_id,
                    line = trimmed,
                    "cline emitted a non-JSON stdout line"
                );
                state.error_text.push_str(trimmed);
                state.error_text.push('\n');
                continue;
            }
        };
        match value.get("type").and_then(Value::as_str) {
            Some("agent_event") => {
                if let Some(event) = value.get("event") {
                    handle_agent_event(event, &mut state, on_chunk)?;
                }
            }
            Some("hook_event") => {
                // 宏参数里别走裸 `Value` 路径:tracing 的展开作用域里也有个
                // `Value`(field trait),会把这儿的 serde 类型挡掉(E0782)。
                let hook = value
                    .get("hookEventName")
                    .and_then(Value::as_str)
                    .unwrap_or("?");
                tracing::debug!(request_id, hook, "cline hook event");
            }
            Some("team_event") => {
                tracing::debug!(request_id, "cline team event (ignored by the relay)");
            }
            Some(other) => {
                tracing::debug!(
                    request_id,
                    kind = other,
                    "cline emitted an unknown event type"
                );
            }
            None => {
                state.error_text.push_str(trimmed);
                state.error_text.push('\n');
            }
        }
    }
    let (exit_code, stderr_text) = process.finish().await;
    let detail = failure_detail(&state, &stderr_text);

    if let Some(message) = state.failed.clone() {
        let mut error = anyhow::anyhow!("cline turn failed: {message}");
        if let Some(failure) = classify_cline_failure(&detail) {
            error = error.context(failure);
        }
        return Err(error);
    }
    if !state.done {
        let mut error = anyhow::anyhow!(
            "cline exited (code {exit_code}) without finishing the turn: {}",
            detail.trim()
        );
        if let Some(failure) = classify_cline_failure(&detail) {
            error = error.context(failure);
        }
        return Err(error);
    }
    if let Some(reason) = state
        .finish_reason
        .as_deref()
        .filter(|reason| *reason != "completed")
    {
        return Err(anyhow::anyhow!(
            "cline finished with reason {reason}: {}",
            detail.trim()
        ));
    }

    flush_buffer(
        &state.reasoning,
        &mut state.reasoning_emitted,
        ChatStreamKind::Reasoning,
        &mut *on_chunk,
        true,
    )?;
    flush_buffer(
        &state.content,
        &mut state.content_emitted,
        ChatStreamKind::Content,
        &mut *on_chunk,
        true,
    )?;
    if state.content.trim().is_empty()
        && state
            .usage
            .as_ref()
            .map(|usage| usage.effective_total_tokens() == 0)
            .unwrap_or(true)
    {
        bail!(
            "cline produced no output (turn completed with zero usage): {}",
            detail.trim()
        );
    }
    let usage = state.usage.clone();
    let mut result = finalize_stream_result(
        state.content,
        state.reasoning,
        usage.clone(),
        Vec::new(),
        false,
    )?;
    result.finish_reason = Some("stop".to_string());
    // cline 只给整轮用量,单次调用口径取不到;上下文表读同一份。
    result.last_request_usage = usage;
    Ok(result)
}

/// 一条 `agent_event` → 顾清影 的流事件/状态。
fn handle_agent_event<F>(event: &Value, state: &mut StreamState, on_chunk: &mut F) -> Result<()>
where
    F: FnMut(ChatStreamChunk) -> Result<()>,
{
    match event.get("type").and_then(Value::as_str) {
        Some("content_start") => match event.get("contentType").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = event.get("text").and_then(Value::as_str) {
                    push_buffered_chunk(
                        &mut state.content,
                        &mut state.content_emitted,
                        ChatStreamKind::Content,
                        text.to_string(),
                        on_chunk,
                    )?;
                }
            }
            Some("reasoning") => {
                if let Some(text) = event.get("reasoning").and_then(Value::as_str) {
                    push_buffered_chunk(
                        &mut state.reasoning,
                        &mut state.reasoning_emitted,
                        ChatStreamKind::Reasoning,
                        text.to_string(),
                        on_chunk,
                    )?;
                }
            }
            Some("tool") => {
                let id = event
                    .get("toolCallId")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let name = event
                    .get("toolName")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if hidden_remote_tool(&name) {
                    return Ok(());
                }
                emit_started(
                    state,
                    &id,
                    &name,
                    event.get("input").cloned().unwrap_or(json!({})),
                    on_chunk,
                )?;
            }
            _ => {}
        },
        Some("content_update") => {
            // 工具进度不进卡片:收口帧给全文,中转不替 cline 做进度渲染。
        }
        Some("content_end") => match event.get("contentType").and_then(Value::as_str) {
            // 增量已经流过了;只有整条没收到任何增量时才用终帧兜底。
            Some("text") => {
                if state.content.is_empty() {
                    if let Some(text) = event.get("text").and_then(Value::as_str) {
                        push_buffered_chunk(
                            &mut state.content,
                            &mut state.content_emitted,
                            ChatStreamKind::Content,
                            text.to_string(),
                            on_chunk,
                        )?;
                    }
                }
            }
            Some("reasoning") => {
                if state.reasoning.is_empty() {
                    if let Some(text) = event.get("reasoning").and_then(Value::as_str) {
                        push_buffered_chunk(
                            &mut state.reasoning,
                            &mut state.reasoning_emitted,
                            ChatStreamKind::Reasoning,
                            text.to_string(),
                            on_chunk,
                        )?;
                    }
                }
            }
            Some("tool") => {
                let id = event
                    .get("toolCallId")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let name = event
                    .get("toolName")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if hidden_remote_tool(name) {
                    return Ok(());
                }
                let output = remote_output_text(event.get("output"));
                let ok = event.get("error").map(Value::is_null).unwrap_or(true);
                emit_finished(state, id, ok, &output, on_chunk)?;
            }
            _ => {}
        },
        Some("usage") => {
            state.usage = usage_from_agent_usage(event).or(state.usage.take());
        }
        Some("done") => {
            state.done = true;
            state.finish_reason = event
                .get("reason")
                .and_then(Value::as_str)
                .map(str::to_string);
            if state.content.is_empty() {
                if let Some(text) = event.get("text").and_then(Value::as_str) {
                    if !text.trim().is_empty() {
                        push_buffered_chunk(
                            &mut state.content,
                            &mut state.content_emitted,
                            ChatStreamKind::Content,
                            text.to_string(),
                            on_chunk,
                        )?;
                    }
                }
            }
            if state.usage.is_none() {
                state.usage = event.get("usage").and_then(usage_from_legacy);
            }
        }
        Some("error") => {
            let message = event
                .pointer("/error/message")
                .and_then(Value::as_str)
                .or_else(|| event.get("error").and_then(Value::as_str))
                .unwrap_or("the cline agent reported an error");
            state.failed = Some(message.to_string());
        }
        Some("notice") => {
            // 同上:hook_event 那条注释说的原因,别在宏参数里写 `Value::…`。
            let notice = event
                .get("noticeType")
                .and_then(Value::as_str)
                .unwrap_or("?");
            let reason = event.get("reason").and_then(Value::as_str).unwrap_or("");
            tracing::debug!(notice, reason, "cline notice");
        }
        _ => {}
    }
    Ok(())
}

fn emit_started<F>(
    state: &mut StreamState,
    id: &str,
    name: &str,
    input: Value,
    on_chunk: &mut F,
) -> Result<()>
where
    F: FnMut(ChatStreamChunk) -> Result<()>,
{
    if state.started_tools.contains_key(id) {
        return Ok(());
    }
    state.started_tools.insert(id.to_string(), name.to_string());
    on_chunk(ChatStreamChunk {
        kind: ChatStreamKind::RemoteToolStarted,
        text: json!({ "id": id, "name": name, "input": input }).to_string(),
    })
}

fn emit_finished<F>(
    state: &mut StreamState,
    id: &str,
    ok: bool,
    output: &str,
    on_chunk: &mut F,
) -> Result<()>
where
    F: FnMut(ChatStreamChunk) -> Result<()>,
{
    let Some(name) = state.started_tools.get(id).cloned() else {
        return Ok(());
    };
    on_chunk(ChatStreamChunk {
        kind: ChatStreamKind::RemoteToolFinished,
        text: json!({
            "id": id,
            "name": name,
            "ok": ok,
            "output": shape_remote_output(&name, output),
        })
        .to_string(),
    })
}

/// 工具结果的正文:字符串原样;对象先摸 `content[].text`(MCP 回包的样子),
/// 摸不到就紧凑 JSON,至少把错误文本留住。
fn remote_output_text(output: Option<&Value>) -> String {
    match output {
        Some(Value::String(text)) => text.clone(),
        Some(value) => {
            let text = value
                .get("content")
                .and_then(Value::as_array)
                .map(|parts| {
                    parts
                        .iter()
                        .filter_map(|part| part.get("text").and_then(Value::as_str))
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default();
            if text.is_empty() {
                value.to_string()
            } else {
                text
            }
        }
        None => String::new(),
    }
}

/// `usage` 事件(`AgentUsageEvent`):一次模型调用/一轮的输入输出与缓存命中。
fn usage_from_agent_usage(event: &Value) -> Option<Usage> {
    let input = event.get("inputTokens").and_then(Value::as_u64)?;
    let output = event
        .get("outputTokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let cached = event
        .get("cacheReadTokens")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .min(input);
    let write = event
        .get("cacheWriteTokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    Some(Usage {
        prompt_tokens: input,
        completion_tokens: output,
        total_tokens: input.saturating_add(output),
        cache_read_tokens: cached,
        cache_write_tokens: write,
        cache_reported: true,
        ..Usage::default()
    })
}

/// `done.usage`(`LegacyAgentUsage`):只有整轮总量,没有单次口径。
fn usage_from_legacy(value: &Value) -> Option<Usage> {
    let input = value.get("inputTokens").and_then(Value::as_u64)?;
    let output = value
        .get("outputTokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let cached = value
        .get("cacheReadTokens")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .min(input);
    let write = value
        .get("cacheWriteTokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    Some(Usage {
        prompt_tokens: input,
        completion_tokens: output,
        total_tokens: input.saturating_add(output),
        cache_read_tokens: cached,
        cache_write_tokens: write,
        cache_reported: true,
        ..Usage::default()
    })
}

/// 失败时的取证:stdout 线索 + stderr 上的错误 JSON(CLI 的
/// `{"type":"error","message":…}` 落在 stderr)。
fn failure_detail(state: &StreamState, stderr_text: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    let stdout_clues = state.error_text.trim();
    if !stdout_clues.is_empty() {
        parts.push(stdout_clues.to_string());
    }
    if let Some(message) = stderr_message(stderr_text) {
        parts.push(message);
    } else {
        let tail = stderr_text.trim();
        if !tail.is_empty() {
            parts.push(tail.to_string());
        }
    }
    parts.join("\n")
}

fn stderr_message(stderr: &str) -> Option<String> {
    stderr.lines().rev().find_map(|line| {
        let value: Value = serde_json::from_str(line.trim()).ok()?;
        value
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|message| !message.trim().is_empty())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(events: &[Value]) -> (StreamState, Vec<ChatStreamChunk>) {
        let mut state = StreamState::default();
        let mut chunks = Vec::new();
        {
            let mut sink = |chunk: ChatStreamChunk| {
                chunks.push(chunk);
                Ok(())
            };
            for event in events {
                handle_agent_event(event, &mut state, &mut sink).unwrap();
            }
        }
        (state, chunks)
    }

    #[test]
    fn deltas_stream_and_end_frames_do_not_duplicate() {
        let (state, chunks) = collect(&[
            json!({ "type": "content_start", "contentType": "reasoning", "reasoning": "想一下" }),
            json!({ "type": "content_start", "contentType": "text", "text": "你" }),
            json!({ "type": "content_start", "contentType": "text", "text": "好" }),
            json!({ "type": "content_end", "contentType": "text", "text": "你好" }),
        ]);
        assert_eq!(state.content, "你好");
        assert_eq!(state.reasoning, "想一下");
        let text: Vec<&str> = chunks
            .iter()
            .filter(|chunk| chunk.kind == ChatStreamKind::Content)
            .map(|chunk| chunk.text.as_str())
            .collect();
        assert_eq!(text, ["你", "好"]);
        assert!(chunks
            .iter()
            .any(|chunk| chunk.kind == ChatStreamKind::Reasoning));
    }

    #[test]
    fn a_turn_without_deltas_falls_back_to_the_end_frame() {
        let (state, chunks) =
            collect(&[json!({ "type": "content_end", "contentType": "text", "text": "整段回答" })]);
        assert_eq!(state.content, "整段回答");
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn tool_events_become_cards_and_ask_question_stays_hidden() {
        let (_, chunks) = collect(&[
            json!({ "type": "content_start", "contentType": "tool", "toolCallId": "t1", "toolName": "read_file", "input": { "path": "a.rs" } }),
            json!({ "type": "content_end", "contentType": "tool", "toolCallId": "t1", "toolName": "read_file", "output": "ok" }),
            json!({ "type": "content_start", "contentType": "tool", "toolCallId": "t2", "toolName": "ask_question", "input": {} }),
        ]);
        let kinds: Vec<ChatStreamKind> = chunks.iter().map(|chunk| chunk.kind).collect();
        assert_eq!(
            kinds,
            [
                ChatStreamKind::RemoteToolStarted,
                ChatStreamKind::RemoteToolFinished
            ]
        );
        let started: Value = serde_json::from_str(&chunks[0].text).unwrap();
        assert_eq!(started["name"], "read_file");
        let finished: Value = serde_json::from_str(&chunks[1].text).unwrap();
        assert_eq!(finished["ok"], true);
        assert_eq!(finished["output"], "ok");
    }

    #[test]
    fn usage_and_done_carry_the_final_counts() {
        let (state, _) = collect(&[
            json!({ "type": "usage", "inputTokens": 100, "outputTokens": 20, "cacheReadTokens": 80 }),
            json!({ "type": "done", "reason": "completed", "text": "done" }),
        ]);
        assert!(state.done);
        assert_eq!(state.finish_reason.as_deref(), Some("completed"));
        let usage = state.usage.unwrap();
        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.completion_tokens, 20);
        assert_eq!(usage.cache_read_tokens, 80);
        assert!(usage.cache_reported);

        let (state, _) = collect(&[
            json!({ "type": "done", "reason": "max_iterations", "usage": { "inputTokens": 5, "outputTokens": 7 } }),
        ]);
        assert_eq!(state.finish_reason.as_deref(), Some("max_iterations"));
        assert_eq!(state.usage.unwrap().total_tokens, 12);
    }

    #[test]
    fn stderr_error_json_is_the_failure_message() {
        let state = StreamState::default();
        let detail = failure_detail(
            &state,
            "{\"ts\":\"2026-09-26T06:58:32.987Z\",\"type\":\"error\",\"message\":\"Unauthorized: re-authenticate your Cline account.\"}\n",
        );
        assert!(detail.contains("Unauthorized"));
        assert!(classify_cline_failure(&detail).is_some());
    }

    #[test]
    fn resume_and_failure_classification() {
        assert!(resume_lost(&anyhow::anyhow!(
            "cline turn failed: Session not found: session_x"
        )));
        assert!(!resume_lost(&anyhow::anyhow!(
            "cline turn failed: rate limit"
        )));
        assert_eq!(
            classify_cline_failure("Unauthorized: re-authenticate").map(|failure| failure.status),
            Some(401)
        );
        assert_eq!(
            classify_cline_failure("usage limit reached").map(|failure| failure.status),
            Some(429)
        );
        assert!(classify_cline_failure("something else").is_none());
    }
}
