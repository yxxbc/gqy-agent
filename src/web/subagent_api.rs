//! 子代理详情:WebUI 详情抽屉读一趟子代理的完整过程与结论。
//!
//! 数据源是审计会话(`kind='subagent'`):会话行记端点与用量,唯一的回合记
//! prompt → 结果,`subagent_trace` 子表记合并后的过程标记。权限口径与后台任务
//! 相同(`job_access`):会话在谁的库里就是谁的,看不到的一律 404。

use crate::web::*;

pub(in crate::web) async fn subagent_detail_http(
    State(state): State<DaemonState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> std::result::Result<Response, ApiError> {
    let identity = require_identity(&headers, &state)?;
    let not_found = || ApiError::new(StatusCode::NOT_FOUND, "subagent not found");
    let owner = state.stores.owner_of_session(&session_id);
    if owner.is_none() || !job_visible_to(&state, &identity, Some(&session_id)) {
        return Err(not_found());
    }
    let store = state.stores.for_session(&session_id);
    let lookup = session_id.clone();
    let (detail, trace) = tokio::task::spawn_blocking(move || -> Result<_> {
        let detail = store.subagent_audit_detail(&lookup)?;
        let trace = match detail {
            Some(_) => store.subagent_trace(&lookup)?,
            None => Vec::new(),
        };
        Ok((detail, trace))
    })
    .await
    .map_err(ApiError::internal)?
    .map_err(ApiError::internal)?;
    let detail = detail.ok_or_else(not_found)?;
    let outcome = match detail.status {
        TurnStatus::Completed => parse_subagent_output(&detail.output),
        _ => SubagentOutcome::default(),
    };
    Ok(Json(json!({
        "session_id": detail.record.session_id,
        "parent_session_id": detail.record.parent_session_id,
        "description": detail.record.name,
        "created_at": detail.record.created_at,
        "updated_at": detail.record.updated_at,
        "status": detail.status.as_str(),
        "state": outcome.state,
        "tier": outcome.tier,
        "prompt": detail.prompt,
        "result": outcome.result,
        "error": outcome.error,
        "stats": outcome.stats,
        "provider_id": detail.provider_id,
        "model": detail.model,
        "usage": {
            "prompt_tokens": detail.prompt_tokens,
            "completion_tokens": detail.completion_tokens,
            "total_tokens": detail.total_tokens,
            "cache_read_tokens": detail.cache_read_tokens,
        },
        "trace": trace,
    }))
    .into_response())
}

#[derive(Debug, Default, PartialEq)]
pub(in crate::web) struct SubagentOutcome {
    pub state: Option<String>,
    pub tier: Option<String>,
    pub stats: Option<Value>,
    pub result: Option<String>,
    pub error: Option<String>,
}

/// 审计回合里存的就是工具返回给主体的那份输出,拆回结论与统计。
///
/// 成功路径是文本形态(08-21 token-diet):`subagent <state> (tier <t>): <描述>`、
/// 可选的档位提示、`stats: {json}`、`result:` 之后到结尾是结论本体。错误路径是
/// `ok:false` 的 JSON。
pub(in crate::web) fn parse_subagent_output(output: &str) -> SubagentOutcome {
    if let Ok(value) = serde_json::from_str::<Value>(output) {
        if value.get("ok").and_then(Value::as_bool) == Some(false) {
            let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
            return SubagentOutcome {
                state: text("state"),
                tier: text("tier"),
                stats: value.get("stats").cloned(),
                result: None,
                error: text("error"),
            };
        }
    }
    let mut outcome = SubagentOutcome::default();
    let (head, result) = match output.split_once("\nresult:\n") {
        Some((head, result)) => (head, Some(result)),
        None => match output.strip_suffix("\nresult:") {
            Some(head) => (head, Some("")),
            None => (output, None),
        },
    };
    outcome.result = result.map(str::to_string);
    for line in head.lines() {
        if let Some(rest) = line.strip_prefix("subagent ") {
            if let Some((state, rest)) = rest.split_once(" (tier ") {
                outcome.state = Some(state.to_string());
                if let Some((tier, _)) = rest.split_once(')') {
                    outcome.tier = Some(tier.to_string());
                }
            }
        } else if let Some(stats) = line.strip_prefix("stats: ") {
            outcome.stats = serde_json::from_str(stats).ok();
        }
    }
    if outcome.result.is_none() && outcome.state.is_none() {
        // 认不出的形态:原样当结论给人看,别吞掉。
        outcome.result = Some(output.to_string());
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_output_splits_into_state_stats_and_result() {
        let output = "subagent completed (tier standard): 查目录\nstats: {\"tool_calls\":3}\nresult:\n找到了\n\n两行";
        let outcome = parse_subagent_output(output);
        assert_eq!(outcome.state.as_deref(), Some("completed"));
        assert_eq!(outcome.tier.as_deref(), Some("standard"));
        assert_eq!(outcome.stats, Some(json!({"tool_calls": 3})));
        assert_eq!(outcome.result.as_deref(), Some("找到了\n\n两行"));
        assert_eq!(outcome.error, None);
    }

    #[test]
    fn tier_notice_line_does_not_break_parsing() {
        let output = "subagent budget_reached (tier lite): x\nlite tier unavailable, fell back\nstats: {}\nresult:\n";
        let outcome = parse_subagent_output(output);
        assert_eq!(outcome.state.as_deref(), Some("budget_reached"));
        assert_eq!(outcome.result.as_deref(), Some(""));
    }

    #[test]
    fn error_json_reports_the_error() {
        let output = r#"{"ok": false, "state": "error", "tier": "cheap", "error": "boom", "stats": {"tool_calls": 0}}"#;
        let outcome = parse_subagent_output(output);
        assert_eq!(outcome.state.as_deref(), Some("error"));
        assert_eq!(outcome.error.as_deref(), Some("boom"));
        assert_eq!(outcome.result, None);
    }

    #[test]
    fn unknown_shape_is_kept_as_the_result() {
        let outcome = parse_subagent_output("something else");
        assert_eq!(outcome.result.as_deref(), Some("something else"));
    }
}
