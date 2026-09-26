//! 上下文分项接口:输入框下方圆环点开的弹窗(2026-09-14,
//! 计划 `docs/plan-is-true/2026-09-14/context-panel.md`)。
//!
//! 点开才算,不随回合事件推:要按会话装配一次 Agent、渲染整份请求再数 token。
//! 耗时写进日志(`context breakdown` 行),超过预期再加缓存。

use crate::web::*;

pub(in crate::web) async fn session_context_breakdown_http(
    State(state): State<DaemonState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> std::result::Result<Json<Value>, ApiError> {
    require_auth(&headers, &state)?;
    require_local_web_session(&state, &headers, &session_id)?;
    let payload = context_breakdown_for(&state, &session_id).map_err(ApiError::internal)?;
    Ok(Json(payload))
}

fn context_breakdown_for(state: &DaemonState, session_id: &str) -> Result<Value> {
    let session_store = state.stores.for_session(session_id);
    let record = session_store
        .session_record(session_id)?
        .with_context(|| format!("session not found: {session_id}"))?;
    // 与 `session_state_for` 同一套装配:会话钉的模型池、dev 会话的作用域。
    let mut config = state.manager.lock().unwrap().config.clone();
    apply_session_model_override_to(&mut config, &session_store, session_id);
    let (config, mode) = if record.persona == crate::state::DEV_PERSONA {
        (config.dev_scoped(), AgentMode::Dev)
    } else {
        (config, AgentMode::Normal)
    };
    let store = session_store.pinned(session_id);

    let started = std::time::Instant::now();
    let breakdown =
        build_session_agent(&config, &state.paths, &store, mode)?.context_breakdown()?;
    tracing::info!(
        session_id,
        elapsed_ms = started.elapsed().as_millis() as u64,
        estimate = breakdown.estimate_tokens,
        "context breakdown"
    );

    let (window, window_assumed) = match config.active_context_window_with_source()? {
        Some((window, source)) => (
            Some(window),
            matches!(source, crate::config::ContextWindowSource::Assumed),
        ),
        None => (None, false),
    };
    let backend = match config.provider(None) {
        Ok(provider) if provider.is_claude_code() => "claude_code",
        Ok(provider) if provider.is_codex() => "codex",
        Ok(provider) if provider.is_antigravity() => "antigravity",
        Ok(provider) if provider.is_cline() => "cline",
        _ => "native",
    };
    // 中转后端:CLI 自己的系统提示词与原生工具 顾清影 看不到,实测减估算就是
    // 「CLI 自带」,含分词器差异。可能为负(CLI 会话被重建过),原样给。
    let cli_overhead_tokens = (backend != "native")
        .then_some(breakdown.measured_tokens)
        .flatten()
        .map(|measured| measured as i64 - breakdown.estimate_tokens as i64);

    Ok(json!({
        "window": window,
        "window_assumed": window_assumed,
        "measured_tokens": breakdown.measured_tokens,
        "estimate_tokens": breakdown.estimate_tokens,
        "categories": breakdown.categories,
        "deferred_tools_tokens": breakdown.deferred_tools_tokens,
        "top": breakdown.top,
        "thresholds": {
            "trim_at_ratio": config.context.trim_at_ratio,
            "compact_force_ratio": config.context.compact_force_ratio,
        },
        "backend": {
            "kind": backend,
            "cli_overhead_tokens": cli_overhead_tokens,
        },
    }))
}
