//! `qq_like`：请她给自己的 QQ 资料卡点赞。
//!
//! **只给发起请求的人本人点**，参数里没有目标号码：否则一句话就能让她去刷
//! 别人的赞，或者被人借来骚扰第三方。
//!
//! **只在白名单里出现**：私聊看 `private_chats.like_whitelist`（QQ 号），群聊看
//! `group_chats.like_whitelist`（群号）。不在名单里的会话根本没有这个工具——
//! 条件在一个会话里不变，工具列表因此逐字节稳定，不破坏缓存（AGENTS §1.1）。
//!
//! **本地记账每日上限**：QQ 普通账号给同一个人每天最多 10 个赞。平台超额会报错，
//! 但让模型反复撞这个错没有意义，这里先挡住。记账在内存里，daemon 重启清零——
//! 那时平台自己的上限仍然兜底。

use super::PlatformTurnContext;
use crate::platform_types::ConversationKind;
use crate::tools::{ToolRegistry, ToolSpec, ToolTrust};
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// QQ 普通账号给同一个人每天最多这么多个赞。
const DAILY_LIMIT: u32 = 10;

/// 本会话能不能请她点赞。
pub(crate) fn like_allowed(context: &PlatformTurnContext) -> bool {
    if context.conversation.platform != "onebot" {
        return false;
    }
    let qq = &context.config.platforms.qq;
    let Ok(id) = context.conversation.conversation_id.parse::<i64>() else {
        return false;
    };
    match context.conversation.kind {
        ConversationKind::Private => qq.private_chats.like_whitelist.contains(&id),
        ConversationKind::Group => qq.group_chats.like_whitelist.contains(&id),
    }
}

/// (机器人账号, 被点赞的人) → (日期, 今天已点)。
fn ledger() -> &'static Mutex<HashMap<(String, String), (String, u32)>> {
    static LEDGER: std::sync::OnceLock<Mutex<HashMap<(String, String), (String, u32)>>> =
        std::sync::OnceLock::new();
    LEDGER.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 今天还能给这个人点几个。
fn remaining_today(key: &(String, String), today: &str) -> u32 {
    let ledger = ledger().lock().unwrap();
    match ledger.get(key) {
        Some((day, used)) if day == today => DAILY_LIMIT.saturating_sub(*used),
        _ => DAILY_LIMIT,
    }
}

fn record(key: (String, String), today: String, times: u32) {
    let mut ledger = ledger().lock().unwrap();
    let entry = ledger.entry(key).or_insert_with(|| (today.clone(), 0));
    if entry.0 != today {
        *entry = (today, 0);
    }
    entry.1 = entry.1.saturating_add(times).min(DAILY_LIMIT);
}

pub(crate) fn register(registry: &mut ToolRegistry, context: Arc<PlatformTurnContext>) {
    if !like_allowed(&context) {
        return;
    }
    registry.register(
        ToolSpec::new(
            "qq_like",
            "Give the person you are talking to likes on their QQ profile card. Use it only when they ask you to like them. The likes always go to the requester, never to anyone else. One person can receive at most 10 likes per day.",
            json!({
                "type": "object",
                "properties": {
                    "times": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": DAILY_LIMIT,
                        "description": "How many likes to give. Defaults to 10."
                    }
                },
                "additionalProperties": false
            }),
            move |args| {
                let context = context.clone();
                async move { like(&context, &args).await }
            },
        )
        .writes()
        .with_trust(ToolTrust::External)
        .with_display_name("QQ点赞"),
    );
}

async fn like(context: &PlatformTurnContext, args: &Value) -> Result<String> {
    let requested = args
        .get("times")
        .and_then(Value::as_u64)
        .unwrap_or(u64::from(DAILY_LIMIT))
        .clamp(1, u64::from(DAILY_LIMIT)) as u32;
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let key = (
        context.conversation.account_id.clone(),
        context.sender_id.clone(),
    );
    let remaining = remaining_today(&key, &today);
    if remaining == 0 {
        return Ok(json!({
            "ok": false,
            "reason": "This person already received today's 10 likes. Try again tomorrow."
        })
        .to_string());
    }
    let times = requested.min(remaining);
    match context
        .adapter
        .send_profile_like(&context.sender_id, times)
        .await
    {
        Ok(()) => {
            record(key, today, times);
            Ok(json!({ "ok": true, "liked": times }).to_string())
        }
        // 平台拒绝（多半是今天已经点满）也回软失败：她转述即可，别当工具故障重试。
        Err(error) => Ok(json!({ "ok": false, "reason": error.to_string() }).to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platforms::tests::shared::test_turn_context;

    #[test]
    fn likes_are_offered_only_in_whitelisted_conversations() {
        let (_temp, mut context, _adapter) = test_turn_context(false);
        // test_turn_context 是私聊，会话号带唯一后缀，改成纯数字才能匹配名单。
        context.conversation.conversation_id = "20000".to_string();
        assert!(!like_allowed(&context));
        context.config.platforms.qq.private_chats.like_whitelist = vec![20000];
        assert!(like_allowed(&context));

        context.conversation.kind = ConversationKind::Group;
        assert!(
            !like_allowed(&context),
            "the private list does not open groups"
        );
        context.config.platforms.qq.group_chats.like_whitelist = vec![20000];
        assert!(like_allowed(&context));
    }

    #[test]
    fn the_daily_ledger_caps_likes_per_person() {
        let key = ("bot-test".to_string(), "user-test".to_string());
        record(key.clone(), "2026-09-25".to_string(), 7);
        assert_eq!(remaining_today(&key, "2026-09-25"), 3);
        record(key.clone(), "2026-09-25".to_string(), 7);
        assert_eq!(remaining_today(&key, "2026-09-25"), 0);
        assert_eq!(
            remaining_today(&key, "2026-09-26"),
            DAILY_LIMIT,
            "a new day resets"
        );
    }
}
