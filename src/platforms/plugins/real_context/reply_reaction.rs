//! 群里正式回复时，按概率给被回复的那条消息贴一个常驻表情。
//!
//! 和 `pending.rs` 里那个临时的「处理中」表情是两回事：那个在回复发出后就撤，
//! 这个一直留着。所以候选里要排除「处理中」用的表情——同一条消息上同一个表情，
//! 撤表情的逻辑会连带把它撤掉。
//!
//! 一条回复可能拆成几条消息发出，每条都会触发 after_send；回合级标记保证只贴一次，
//! 并且只掷一次骰子（不因为拆成几条而提高概率）。

use crate::config::RealContextPluginSettings;
use crate::platforms::{PlatformInboundEventKind, PlatformTurnContext};
use serde_json::Value;

const DONE_KEY: &str = "real_context.reply_reaction_done";

pub(super) async fn react_after_reply(
    context: &PlatformTurnContext,
    settings: &RealContextPluginSettings,
) {
    if context.plugin_value(DONE_KEY).is_some() {
        return;
    }
    context.set_plugin_value(DONE_KEY, Value::Bool(true));
    let Some(event) = context.inbound_event() else {
        return;
    };
    if event.kind != PlatformInboundEventKind::Message || event.message_id.is_empty() {
        return;
    }
    let Some(emoji) = pick_emoji(settings, rand::random::<f64>(), rand::random::<u64>()) else {
        return;
    };
    if let Err(error) = context
        .set_message_reaction(&event.message_id, &emoji.to_string(), true)
        .await
    {
        tracing::debug!(target: "gqy::qq", error = %error, "{}", crate::i18n::text("QQ reply reaction could not be added", "QQ 回复表情没能贴上"));
    }
}

/// `roll` 决定贴不贴，`seed` 决定贴哪个。拆出来是为了测试不依赖随机数。
fn pick_emoji(settings: &RealContextPluginSettings, roll: f64, seed: u64) -> Option<u32> {
    if roll >= settings.reply_reaction_probability {
        return None;
    }
    let temporary = |id: &u32| {
        settings.active_reply_reaction_enable
            && settings.active_reply_reaction_emoji_ids.contains(id)
    };
    let choices = settings
        .reply_reaction_emoji_ids
        .iter()
        .copied()
        .filter(|id| !temporary(id))
        .collect::<Vec<_>>();
    if choices.is_empty() {
        return None;
    }
    Some(choices[(seed % choices.len() as u64) as usize])
}

#[cfg(test)]
mod tests {
    use super::pick_emoji;
    use crate::config::RealContextPluginSettings;

    #[test]
    fn picks_by_probability_and_never_the_temporary_emoji() {
        let mut settings = RealContextPluginSettings::default();
        settings.reply_reaction_probability = 0.3;
        settings.reply_reaction_emoji_ids = vec![289, 76];
        settings.active_reply_reaction_enable = true;
        settings.active_reply_reaction_emoji_ids = vec![289];

        assert_eq!(
            pick_emoji(&settings, 0.5, 0),
            None,
            "roll above the probability"
        );
        for seed in 0..8 {
            assert_eq!(
                pick_emoji(&settings, 0.1, seed),
                Some(76),
                "289 is the temporary one"
            );
        }

        settings.reply_reaction_probability = 0.0;
        assert_eq!(pick_emoji(&settings, 0.0, 0), None, "0 means never");
    }
}
