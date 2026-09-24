//! 私聊主动找人（`qq_private_initiative`，默认关闭）。
//!
//! 流程：私聊里她正式回复之后开始倒计时；安静满 `quiet_minutes` 就问一次规划模型
//! 「要不要、什么时候、想聊什么」（planner.rs）；有计划就记进账本（store.rs）；
//! daemon 级后台循环到点后以对方为发起者开一个自发回合（`onebot::wake_private_for_initiative`），
//! 由她用人格口吻写这条消息。
//!
//! 守住的规则（都在代码里，不求模型自觉）：
//! - 只对管理员与私聊白名单（静态 ∪ 动态授权），执行时再核一次；
//! - 对方先说话：倒计时作废、已排的计划取消；
//! - 睡眠时间不发；错过执行窗口就作废，不补发；每人每天有上限；
//! - 先记账再发起回合：失败不重试，不会因为一次失败连发。

mod planner;
mod store;

use super::{PlatformPlugin, PluginDescriptor};
use crate::config::{
    AppConfig, QqPrivateInitiativePluginSettings, QQ_PRIVATE_INITIATIVE_PLUGIN_ID,
};
use crate::paths::GqyPaths;
use crate::platforms::access_control::{has_dynamic_access, AccessPermission};
use crate::platforms::{
    ConversationKind, OutboundBody, OutboundMessage, OutboundOrigin, OutboundSegment,
    PlatformInboundEvent, PlatformInboundEventKind, PlatformTurnContext, SendReceipt,
};
use crate::state::StateStore;
use anyhow::Result;
use futures_util::future::BoxFuture;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const TICK_SECONDS: u64 = 30;
const SCHEDULED_KEY: &str = "qq_private_initiative.scheduled";

pub(super) struct PrivateInitiativePlugin;

impl PrivateInitiativePlugin {
    pub(super) fn new() -> Self {
        Self
    }
}

/// 每个私聊一个代数：对方说话、或者又回复了一次，旧的倒计时就作废。
fn generations() -> &'static Mutex<HashMap<String, u64>> {
    static GENERATIONS: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    GENERATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn bump(key: &str) -> u64 {
    let mut generations = generations().lock().unwrap();
    let generation = generations.entry(key.to_string()).or_insert(0);
    *generation += 1;
    *generation
}

fn current(key: &str) -> u64 {
    generations().lock().unwrap().get(key).copied().unwrap_or(0)
}

fn settings_in(config: &AppConfig) -> Option<QqPrivateInitiativePluginSettings> {
    let instance = config
        .platforms
        .qq
        .plugins
        .get(QQ_PRIVATE_INITIATIVE_PLUGIN_ID)?;
    if !instance.enabled_or(false) {
        return None;
    }
    QqPrivateInitiativePluginSettings::from_instance(instance).ok()
}

fn reply_text(message: &OutboundMessage) -> String {
    let OutboundBody::Segments(segments) = &message.body else {
        return String::new();
    };
    segments
        .iter()
        .filter_map(|segment| match segment {
            OutboundSegment::Text(text) | OutboundSegment::Markdown(text) => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

/// 执行前再核一次对象资格：计划排下之后，名单可能已经改了。
fn eligible(config: &AppConfig, state_store: &StateStore, account: &str, user: &str) -> bool {
    let qq = &config.platforms.qq;
    let listed = user
        .parse::<i64>()
        .ok()
        .is_some_and(|id| qq.admin_users.contains(&id) || qq.private_chats.whitelist.contains(&id));
    listed
        || has_dynamic_access(state_store, account, AccessPermission::Administrator, user)
        || has_dynamic_access(
            state_store,
            account,
            AccessPermission::PrivateWhitelist,
            user,
        )
}

impl PlatformPlugin for PrivateInitiativePlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: QQ_PRIVATE_INITIATIVE_PLUGIN_ID,
            priority: 10,
            // 会主动给人发消息：默认关，要人在设置里打开。
            default_enabled: false,
        }
    }

    fn observe_ingress<'a>(
        &'a self,
        paths: &'a GqyPaths,
        _config: &'a AppConfig,
        event: &'a PlatformInboundEvent,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            // 只认对方自己发的私聊消息；机器人自己的消息回显不算。
            if event.kind != PlatformInboundEventKind::Message
                || event.conversation.kind != ConversationKind::Private
                || event.sender_id != event.conversation.conversation_id
            {
                return Ok(());
            }
            let account = &event.conversation.account_id;
            let user = &event.conversation.conversation_id;
            bump(&store::key(account, user));
            if store::cancel(paths, account, user)? {
                tracing::info!(target: "gqy::qq", %user, "private initiative cancelled: they wrote first");
            }
            Ok(())
        })
    }

    fn after_send<'a>(
        &'a self,
        context: &'a PlatformTurnContext,
        message: &'a OutboundMessage,
        _receipt: &'a SendReceipt,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            if message.origin != OutboundOrigin::FinalReply
                || context.conversation.kind != ConversationKind::Private
                || context.conversation.platform != "onebot"
                || context.plugin_value(SCHEDULED_KEY).is_some()
                || !context.admin_or_private_whitelisted()
            {
                return Ok(());
            }
            // 一条回复可能拆成几条发出：每回合只排一次倒计时。
            context.set_plugin_value(SCHEDULED_KEY, Value::Bool(true));
            let Some(settings) = settings_in(&context.config) else {
                return Ok(());
            };
            let exchange = planner::Exchange {
                account: context.conversation.account_id.clone(),
                user: context.conversation.conversation_id.clone(),
                user_text: context
                    .inbound_event()
                    .map(|event| event.text.clone())
                    .unwrap_or_default(),
                reply_text: reply_text(message),
            };
            let key = store::key(&exchange.account, &exchange.user);
            let generation = bump(&key);
            let config = context.config.clone();
            let paths = context.paths.clone();
            let state_store = context.state_store.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(settings.quiet_minutes * 60)).await;
                if current(&key) != generation {
                    return;
                }
                match planner::plan(&config, &paths, &state_store, &settings, &exchange).await {
                    // 规划期间对方又说话了：这份计划作废。
                    Ok(Some(plan)) if current(&key) == generation => {
                        tracing::info!(target: "gqy::qq", user = %plan.user, at = plan.at, "private initiative planned");
                        if let Err(error) = store::save_plan(&paths, plan) {
                            tracing::warn!(target: "gqy::qq", %error, "saving the private initiative plan failed");
                        }
                    }
                    Ok(_) => {}
                    Err(error) => {
                        tracing::warn!(target: "gqy::qq", %error, "private initiative planning failed")
                    }
                }
            });
            Ok(())
        })
    }
}

/// daemon 级后台循环：到点的计划逐个核对后发起回合。
pub(crate) fn spawn_private_initiative_worker(state: crate::runtime::DaemonState) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(TICK_SECONDS)).await;
            // 锁内只读开关与设置；有到点的计划才整份拷贝配置。
            let settings = {
                let manager = state.manager.lock().unwrap();
                if !manager.config.platforms.qq.enabled {
                    continue;
                }
                settings_in(&manager.config)
            };
            let Some(settings) = settings else {
                continue;
            };
            let now = chrono::Local::now();
            let window = i64::try_from(settings.fire_window_minutes).unwrap_or(15) * 60;
            let due = match store::take_due(&state.paths, now.timestamp(), window) {
                Ok(due) if !due.is_empty() => due,
                Ok(_) => continue,
                Err(error) => {
                    tracing::warn!(target: "gqy::qq", %error, "reading private initiative plans failed");
                    continue;
                }
            };
            let config = state.manager.lock().unwrap().config.clone();
            let today = now.date_naive().to_string();
            for plan in due {
                if !eligible(&config, &state.state_store, &plan.account, &plan.user)
                    || config.platforms.qq.is_sleeping_at(now.time())
                {
                    continue;
                }
                match store::sent_today(&state.paths, &plan.account, &plan.user, &today) {
                    Ok(count) if count < settings.max_per_day => {}
                    _ => continue,
                }
                // 先记账再发起：失败不重试，也就不会因为一次失败连发。
                if let Err(error) =
                    store::record_sent(&state.paths, &plan.account, &plan.user, &today)
                {
                    tracing::warn!(target: "gqy::qq", %error, "recording a private initiative failed");
                    continue;
                }
                if let Err(error) = crate::platforms::onebot::wake_private_for_initiative(
                    &state,
                    &plan.account,
                    &plan.user,
                    &plan.topic,
                )
                .await
                {
                    tracing::warn!(target: "gqy::qq", %error, user = %plan.user, "private initiative turn failed");
                }
            }
        }
    });
}
