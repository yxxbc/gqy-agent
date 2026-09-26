//! 旧 iMessage 桥接的偏好搬家（一次性）。
//!
//! 旧桥接把每个联系人的当前话题、钉的模型、暂停状态记在
//! `state/imessage-contacts.json`（`{"<联系人>": {"topic": N, "model": "…", "paused": bool}}`）。
//! 这个对话第一次经连接器进来、还没有会话绑定时搬一次：把绑定指到当前话题的
//! 会话，模型写成会话级覆盖，暂停状态写进偏好。搬完记一笔，不再读这个文件。

use super::commands::{binding_key, topic_name, update_prefs, ContactPrefs};
use super::inbound::session_name;
use crate::config::ActiveProviderModelConfig;
use crate::platforms::*;
use serde::Deserialize;

const LEGACY_PLATFORM: &str = "imessage";
const LEGACY_FILE: &str = "imessage-contacts.json";

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct LegacyPrefs {
    topic: usize,
    model: String,
    paused: bool,
}

pub(super) fn migrate(
    state: &DaemonState,
    conversation: &PlatformConversation,
    persona: &str,
    prefs: &ContactPrefs,
) {
    if conversation.platform != LEGACY_PLATFORM || prefs.legacy_migrated {
        return;
    }
    let key = binding_key(conversation, persona);
    if let Err(error) = migrate_inner(state, conversation, persona, &key) {
        tracing::warn!(target: "gqy::platform", error = %error, "legacy iMessage bridge preferences could not be migrated");
    }
    let _ = update_prefs(state, conversation, |prefs| prefs.legacy_migrated = true);
}

fn migrate_inner(
    state: &DaemonState,
    conversation: &PlatformConversation,
    persona: &str,
    key: &crate::state::PlatformSessionBindingKey,
) -> Result<()> {
    if state
        .state_store
        .find_platform_session_binding(key)?
        .is_some()
    {
        return Ok(());
    }
    let path = state.paths.state_dir.join(LEGACY_FILE);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Ok(());
    };
    let all: std::collections::HashMap<String, LegacyPrefs> = serde_json::from_str(&raw)?;
    let Some(legacy) = all.get(&conversation.conversation_id) else {
        return Ok(());
    };
    let base = session_name(&conversation.platform, &conversation.conversation_id);
    let name = topic_name(&base, legacy.topic.max(1));
    let Some(record) = state.state_store.find_session_by_name(persona, &name)? else {
        return Ok(());
    };
    state
        .state_store
        .bind_platform_session(key, &record.session_id)?;
    if legacy.paused {
        update_prefs(state, conversation, |prefs| prefs.paused = true)?;
    }
    let model = legacy.model.trim();
    if !model.is_empty()
        && state
            .state_store
            .session_model_override(&record.session_id)?
            .is_none()
    {
        let choice = state
            .manager
            .lock()
            .unwrap()
            .config
            .text_provider_model_choices()
            .into_iter()
            .find(|choice| choice.model == model);
        if let Some(choice) = choice {
            state.state_store.set_session_model_override(
                &record.session_id,
                Some(&[ActiveProviderModelConfig {
                    provider_id: choice.provider_id,
                    model: choice.model,
                }]),
            )?;
        }
    }
    tracing::info!(target: "gqy::platform", session = %record.name, "legacy iMessage bridge preferences migrated");
    Ok(())
}
