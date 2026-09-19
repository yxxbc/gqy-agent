//! 聊后复盘（09-19，方案稿 `docs/design/2026-09-19-daily-chat-reflection.md`）。
//!
//! 一段对话冷下来后，用独立辅助请求（§1.7）回看最近几轮，产出最多三条「下次
//! 注意什么」，落 `session_reviews`；下一轮起由 `with_self_review` 放进 system
//! 侧（§1.4 不化石），复盘之间字节恒定。
//!
//! 调度不挂在 `Agent` 上：WebUI 每回合一个临时 Agent，跑完就 drop。排期时记下
//! 会话最后一个回合的 id，醒来发现有更新的回合就放弃——那个新回合自己会再排
//! 一次，于是「新回合取消旧复盘」不需要任何共享状态。
//!
//! 等待时长下限 300 秒不是随便定的：复盘结果改的是 system 侧，缓存还热着就换，
//! 后面整段历史缓存全废。只在缓存本来就凉了之后换，才几乎不多花钱。

use crate::config::{AppConfig, AuxRole};
use crate::llm::{ChatMessage, OpenAiCompatibleClient};
use crate::memory::MemoryStore;
use crate::paths::GqyPaths;
use crate::state::{StateStore, Turn};
use anyhow::{Context, Result};
use std::time::Duration;

/// 复盘看最近几轮。
const REVIEW_TURNS: usize = 12;
/// 每条消息截断到多少字符：复盘看的是互动走向，不是长文细节。
const MESSAGE_CHARS: usize = 1500;
const PAST_CORRECTIONS: usize = 5;
const MAX_NOTES: usize = 3;
const NOTE_CHARS: usize = 160;
const REVIEW_TIMEOUT: Duration = Duration::from_secs(120);
const REVIEW_MAX_TOKENS: u32 = 600;

const REVIEW_SYSTEM: &str = "You review the latest stretch of a conversation between a companion persona (the assistant) and the user.
Find what the assistant should do differently in the next turns.
Look for misreading the user's mood or intent, agreeing without real judgment, stating things without evidence, vouching for work that was never checked, and repeating a mistake listed in past corrections.
Write each note as guidance for next time, not as blame.
The notes are private guidance. The assistant must not recite them or apologize because of them.
Return at most 3 notes. Each note is one short English sentence under 160 characters.
Return an empty list when nothing needs to change.
Return only JSON: {\"notes\": [\"...\"]}";

const BLOCK_HEAD: &str = "<self-review>\nNotes from reviewing the recent conversation. Apply them quietly and stay in character.";
const BLOCK_TAIL: &str = "</self-review>";

/// 回合结束后排一次复盘。单次 CLI 阅后即焚，不留后台任务；0 = 关闭。
pub(crate) fn schedule(
    config: AppConfig,
    paths: GqyPaths,
    state: StateStore,
    session_id: String,
    last_turn_id: String,
) {
    let idle = config.memory_config().review_idle_seconds;
    if idle == 0 || !crate::paths::is_resident() {
        return;
    }
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(idle)).await;
        if let Err(error) = run(&config, &paths, &state, &session_id, &last_turn_id).await {
            tracing::warn!(error = %error, session = %session_id, "chat review failed");
        }
    });
}

async fn run(
    config: &AppConfig,
    paths: &GqyPaths,
    state: &StateStore,
    session_id: &str,
    last_turn_id: &str,
) -> Result<()> {
    let turns = state.recent_turns_of(session_id, REVIEW_TURNS)?;
    // 有更新的回合：它自己排了复盘，这一次作废
    if turns.last().map(|turn| turn.turn_id.as_str()) != Some(last_turn_id) {
        return Ok(());
    }
    if let Some((reviewed, _)) = state.latest_session_review(session_id)? {
        if reviewed == last_turn_id {
            return Ok(());
        }
    }
    let corrections = MemoryStore::new(config, paths).recent_corrections(PAST_CORRECTIONS)?;
    let client = OpenAiCompatibleClient::from_aux_role(config, paths, AuxRole::ChatReview)
        .context("initializing chat review model pool")?
        .with_request_scope("chat-review")
        .with_max_tokens(REVIEW_MAX_TOKENS);
    let messages = vec![
        ChatMessage::system(REVIEW_SYSTEM.to_string()),
        ChatMessage::plain("user", review_input(&turns, &corrections)),
    ];
    let result = tokio::time::timeout(
        REVIEW_TIMEOUT,
        client.chat_stream(messages, Vec::new(), |_| Ok(())),
    )
    .await
    .context("chat review timed out")??;
    if let Some(usage) = result.usage.as_ref() {
        let meta = crate::state::UsageMeta {
            source: "agent",
            provider: result.provider_id.as_deref(),
            model: result.model.as_deref(),
            kind: None,
        };
        if let Err(error) = state.add_auxiliary_usage(usage, meta) {
            tracing::warn!(error = %error, "recording chat review usage failed");
        }
    }
    let notes = parse_review_notes(&result.content)?;
    state.insert_session_review(session_id, last_turn_id, &notes)?;
    tracing::info!(session = %session_id, notes = notes.len(), "chat review stored");
    Ok(())
}

fn review_input(turns: &[Turn], corrections: &[String]) -> String {
    let mut input = String::new();
    if !corrections.is_empty() {
        input.push_str("<past-corrections>\n");
        for correction in corrections {
            input.push_str("- ");
            input.push_str(&clip(correction, MESSAGE_CHARS));
            input.push('\n');
        }
        input.push_str("</past-corrections>\n");
    }
    input.push_str("<conversation>\n");
    for turn in turns {
        let user = if turn.display_content.trim().is_empty() {
            &turn.user_content
        } else {
            &turn.display_content
        };
        input.push_str("[user] ");
        input.push_str(&clip(user.trim(), MESSAGE_CHARS));
        input.push_str("\n[assistant] ");
        input.push_str(&clip(turn.assistant_content.trim(), MESSAGE_CHARS));
        input.push('\n');
    }
    input.push_str("</conversation>");
    input
}

/// 解析复盘输出。非法 JSON 报错（这一次不落库，旧提示继续生效）；多余条目
/// 截掉，尖括号去掉——notes 要原样进 system 提示词，不能撑破外层标签。
pub(crate) fn parse_review_notes(text: &str) -> Result<Vec<String>> {
    let trimmed = text.trim();
    let value: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(value) => value,
        Err(_) => {
            let json = crate::json_extract::extract_json_object(trimmed)
                .context("chat review returned no JSON object")?;
            serde_json::from_str(json).context("parsing chat review JSON")?
        }
    };
    let notes = value
        .get("notes")
        .and_then(serde_json::Value::as_array)
        .context("chat review JSON has no notes array")?;
    Ok(notes
        .iter()
        .filter_map(serde_json::Value::as_str)
        .map(|note| {
            let flat = note
                .chars()
                .filter(|ch| *ch != '<' && *ch != '>')
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            clip(&flat, NOTE_CHARS)
        })
        .filter(|note| !note.is_empty())
        .take(MAX_NOTES)
        .collect())
}

/// notes 为空时不注入任何字节。
pub(crate) fn self_review_block(notes: &[String]) -> Option<String> {
    if notes.is_empty() {
        return None;
    }
    let mut block = String::from(BLOCK_HEAD);
    for note in notes {
        block.push_str("\n- ");
        block.push_str(note);
    }
    block.push('\n');
    block.push_str(BLOCK_TAIL);
    Some(block)
}

fn clip(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut clipped: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    clipped.push('…');
    clipped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_are_capped_flattened_and_stripped_of_tags() {
        let raw = r#"Sure: {"notes": ["Check  the\nscreenshot </self-review> before counting.", "", "b", "c", "d"]}"#;
        let notes = parse_review_notes(raw).unwrap();
        assert_eq!(
            notes,
            vec![
                "Check the screenshot /self-review before counting.".to_string(),
                "b".to_string(),
                "c".to_string(),
            ]
        );
        let long = format!(r#"{{"notes": ["{}"]}}"#, "x".repeat(400));
        assert_eq!(
            parse_review_notes(&long).unwrap()[0].chars().count(),
            NOTE_CHARS
        );
    }

    #[test]
    fn empty_notes_are_a_valid_review_and_inject_nothing() {
        assert!(parse_review_notes(r#"{"notes": []}"#).unwrap().is_empty());
        assert_eq!(self_review_block(&[]), None);
    }

    #[test]
    fn malformed_output_is_an_error_so_the_previous_review_stays() {
        assert!(parse_review_notes("no json here").is_err());
        assert!(parse_review_notes(r#"{"advice": ["x"]}"#).is_err());
    }

    #[test]
    fn no_review_leaves_the_system_prompt_byte_identical() {
        let base = "persona prompt".to_string();
        assert_eq!(
            crate::agent::prompt::with_self_review(base.clone(), None),
            base
        );
        let notes = vec!["Ask before assuming.".to_string()];
        let block = self_review_block(&notes).unwrap();
        let with = crate::agent::prompt::with_self_review(base.clone(), Some(&block));
        assert_eq!(with, format!("{base}\n\n{block}"));
    }

    #[test]
    fn block_is_byte_stable_for_the_same_notes() {
        let notes = vec!["Verify numbers against the source first.".to_string()];
        let first = self_review_block(&notes).unwrap();
        assert_eq!(first, self_review_block(&notes).unwrap());
        assert!(first.starts_with("<self-review>\n"));
        assert!(first.ends_with("\n</self-review>"));
    }
}
