//! 资料卡点赞插件（`qq_profile_like`，默认开启）：请她给自己的 QQ 资料卡点赞。
//!
//! - **只给发起请求的人本人点**，工具参数里没有目标号码：否则一句话就能让她去刷
//!   别人的赞，或者被人借来骚扰第三方。
//! - **谁能要**：主人号在哪都能要（权限最高）；其他人看插件设置里的两份名单——
//!   私聊按 QQ 号（`private_whitelist`），群聊按群号（`group_whitelist`）。不满足
//!   条件的会话根本没有这个工具；条件在一个会话里不变，工具列表因此逐字节稳定，
//!   不破坏缓存（AGENTS §1.1）。
//! - **本地记账每日上限**：QQ 普通账号给同一个人每天最多 10 个赞。平台超额会报错，
//!   但让模型反复撞这个错没有意义，这里先挡住。记账在内存里，daemon 重启清零——
//!   那时平台自己的上限仍然兜底。

use super::{PlatformPlugin, PluginDescriptor};
use crate::config::{AppConfig, QqProfileLikePluginSettings, QQ_PROFILE_LIKE_PLUGIN_ID};
use crate::platform_types::ConversationKind;
use crate::platforms::PlatformTurnContext;
use crate::tools::{ToolRegistry, ToolSpec, ToolTrust};
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// QQ 普通账号给同一个人每天最多这么多个赞。
const DAILY_LIMIT: u32 = 10;

pub(super) struct ProfileLikePlugin;

impl ProfileLikePlugin {
    pub(super) fn new() -> Self {
        Self
    }
}

fn settings(config: &AppConfig) -> QqProfileLikePluginSettings {
    config
        .platforms
        .qq
        .plugins
        .get(QQ_PROFILE_LIKE_PLUGIN_ID)
        .and_then(|instance| QqProfileLikePluginSettings::from_instance(instance).ok())
        .unwrap_or_default()
}

/// 本会话的发起者能不能请她点赞（插件开关由注册表先行过滤）。
fn like_allowed(context: &PlatformTurnContext) -> bool {
    if context.conversation.platform != "onebot" {
        return false;
    }
    let qq = &context.config.platforms.qq;
    if context
        .sender_id
        .parse::<i64>()
        .is_ok_and(|sender| qq.is_owner(sender))
    {
        return true;
    }
    let Ok(id) = context.conversation.conversation_id.parse::<i64>() else {
        return false;
    };
    let lists = settings(&context.config);
    match context.conversation.kind {
        ConversationKind::Private => lists.private_whitelist.contains(&id),
        ConversationKind::Group => lists.group_whitelist.contains(&id),
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

impl PlatformPlugin for ProfileLikePlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: QQ_PROFILE_LIKE_PLUGIN_ID,
            priority: 100,
            // 只在有人明确要赞时才动作，默认开；名单为空时只有主人能用。
            default_enabled: true,
        }
    }

    fn register_tools(
        &self,
        registry: &mut ToolRegistry,
        context: Arc<PlatformTurnContext>,
    ) -> Result<()> {
        if !like_allowed(&context) {
            return Ok(());
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
        Ok(())
    }
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

    fn set_lists(context: &mut PlatformTurnContext, private: Vec<i64>, group: Vec<i64>) {
        let instance = context
            .config
            .platforms
            .qq
            .plugins
            .entry(QQ_PROFILE_LIKE_PLUGIN_ID.to_string())
            .or_default();
        instance
            .settings
            .insert("private_whitelist".into(), json!(private));
        instance
            .settings
            .insert("group_whitelist".into(), json!(group));
    }

    #[test]
    fn likes_follow_the_plugin_lists_and_the_owner_is_always_allowed() {
        let (_temp, mut context, _adapter) = test_turn_context(false);
        // test_turn_context 是私聊，发起者 20000；会话号带唯一后缀，改成纯数字才能匹配名单。
        context.conversation.conversation_id = "20000".to_string();
        assert!(!like_allowed(&context));
        set_lists(&mut context, vec![20000], vec![]);
        assert!(like_allowed(&context));

        context.conversation.kind = ConversationKind::Group;
        context.conversation.conversation_id = "500".to_string();
        assert!(
            !like_allowed(&context),
            "the private list does not open groups"
        );
        set_lists(&mut context, vec![], vec![500]);
        assert!(like_allowed(&context));

        set_lists(&mut context, vec![], vec![]);
        context.config.platforms.qq.owner_users = vec![20000];
        assert!(like_allowed(&context), "owners need no list");
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
