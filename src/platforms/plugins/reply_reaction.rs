//! 回复贴表情插件（`qq_reply_reaction`，默认开启）：群里她回复一条消息时，按概率
//! 在被回复的那条消息上贴一个常驻表情。
//!
//! 和真实上下文插件那个临时的「处理中」表情是两回事：那个在回复发出后就撤，这个
//! 一直留着。所以候选里要排除「处理中」用的表情——同一条消息上同一个表情，撤表情
//! 的逻辑会连带把它撤掉。
//!
//! 回复可能是最终正文，也可能由工具发出（`FinalReply` / `Tool` 两种出站来源都算），
//! 还可能拆成几条；每条都会触发 after_send。回合级标记保证只贴一次，并且只掷一次
//! 骰子（不因为拆成几条而提高概率）。

use super::{PlatformPlugin, PluginDescriptor};
use crate::config::{
    AppConfig, QqReplyReactionPluginSettings, RealContextPluginSettings,
    QQ_REPLY_REACTION_PLUGIN_ID, REAL_CONTEXT_PLUGIN_ID,
};
use crate::platforms::{
    ConversationKind, OutboundMessage, OutboundOrigin, PlatformInboundEventKind,
    PlatformTurnContext, SendReceipt,
};
use anyhow::Result;
use futures_util::future::BoxFuture;
use serde_json::Value;

const DONE_KEY: &str = "qq_reply_reaction.done";

pub(super) struct ReplyReactionPlugin;

impl ReplyReactionPlugin {
    pub(super) fn new() -> Self {
        Self
    }
}

fn settings(config: &AppConfig) -> QqReplyReactionPluginSettings {
    config
        .platforms
        .qq
        .plugins
        .get(QQ_REPLY_REACTION_PLUGIN_ID)
        .and_then(|instance| QqReplyReactionPluginSettings::from_instance(instance).ok())
        .unwrap_or_default()
}

/// 真实上下文插件开着「处理中」表情时，它用的那几个表情 ID。
fn temporary_emoji_ids(config: &AppConfig) -> Vec<u32> {
    let real_context = config
        .platforms
        .qq
        .plugins
        .get(REAL_CONTEXT_PLUGIN_ID)
        .and_then(|instance| RealContextPluginSettings::from_instance(instance).ok())
        .unwrap_or_default();
    if real_context.active_reply_reaction_enable {
        real_context.active_reply_reaction_emoji_ids
    } else {
        Vec::new()
    }
}

/// `roll` 决定贴不贴，`seed` 决定贴哪个。拆出来是为了测试不依赖随机数。
fn pick_emoji(
    settings: &QqReplyReactionPluginSettings,
    temporary: &[u32],
    roll: f64,
    seed: u64,
) -> Option<u32> {
    if roll >= settings.probability {
        return None;
    }
    let choices = settings
        .emoji_ids
        .iter()
        .copied()
        .filter(|id| !temporary.contains(id))
        .collect::<Vec<_>>();
    if choices.is_empty() {
        return None;
    }
    Some(choices[(seed % choices.len() as u64) as usize])
}

impl PlatformPlugin for ReplyReactionPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: QQ_REPLY_REACTION_PLUGIN_ID,
            priority: 100,
            default_enabled: true,
        }
    }

    fn after_send<'a>(
        &'a self,
        context: &'a PlatformTurnContext,
        message: &'a OutboundMessage,
        _receipt: &'a SendReceipt,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            if !matches!(
                message.origin,
                OutboundOrigin::FinalReply | OutboundOrigin::Tool
            ) || context.conversation.kind != ConversationKind::Group
                || context.plugin_value(DONE_KEY).is_some()
            {
                return Ok(());
            }
            context.set_plugin_value(DONE_KEY, Value::Bool(true));
            let Some(event) = context.inbound_event() else {
                return Ok(());
            };
            if event.kind != PlatformInboundEventKind::Message || event.message_id.is_empty() {
                return Ok(());
            }
            let chosen = pick_emoji(
                &settings(&context.config),
                &temporary_emoji_ids(&context.config),
                rand::random::<f64>(),
                rand::random::<u64>(),
            );
            let Some(emoji) = chosen else {
                return Ok(());
            };
            if let Err(error) = context
                .set_message_reaction(&event.message_id, &emoji.to_string(), true)
                .await
            {
                tracing::warn!(target: "gqy::qq", error = %error, "{}", crate::i18n::text("QQ reply reaction could not be added", "QQ 回复表情没能贴上"));
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::pick_emoji;
    use crate::config::QqReplyReactionPluginSettings;

    #[test]
    fn picks_by_probability_and_never_the_temporary_emoji() {
        let settings = QqReplyReactionPluginSettings {
            probability: 0.3,
            emoji_ids: vec![289, 76],
        };
        assert_eq!(
            pick_emoji(&settings, &[289], 0.5, 0),
            None,
            "roll above the probability"
        );
        for seed in 0..8 {
            assert_eq!(
                pick_emoji(&settings, &[289], 0.1, seed),
                Some(76),
                "289 is the temporary one"
            );
        }
        let never = QqReplyReactionPluginSettings {
            probability: 0.0,
            ..settings
        };
        assert_eq!(pick_emoji(&never, &[], 0.0, 0), None, "0 means never");
    }
}
