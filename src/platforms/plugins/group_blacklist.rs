//! 群聊黑名单（`qq_group_blacklist`，默认开启）：让她在群里不理某个人。
//!
//! - **效果**：被拉黑的人说的话照常进群聊记录与上下文（她仍看得懂别人在聊什么），
//!   但不进任何触发判断——@她、关键词、接话、主动插话都不回。拦截点在
//!   `onebot::dispatch` 里 `observe_inbound` 之后、`decide_trigger` 之前，所以
//!   real_context 的「处理中」表情与判官调用都不会发生。
//! - **范围**：默认只在当前群；`scope=all` 在这个机器人账号的所有群里都不理。
//! - **期限**：可选 `hours`，到期自动失效；不给就是永久。
//! - **谁能拉黑**：只有顾清影的管理员（工具只注册给管理员）。管理员与机器人自己
//!   不能被拉黑。
//!
//! 账本存在 `state/qq_group_blacklist.json`（`json_ledger`），每条群消息查一次，只碰内存。

use super::json_ledger;
use super::{PlatformPlugin, PluginDescriptor};
use crate::config::AppConfig;
use crate::paths::GqyPaths;
use crate::platforms::access_control::{has_dynamic_access, AccessPermission};
use crate::platforms::{ConversationKind, PlatformTurnContext};
use crate::tools::{ToolRegistry, ToolSpec};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

pub(crate) const QQ_GROUP_BLACKLIST_PLUGIN_ID: &str = "qq_group_blacklist";
const FILE_NAME: &str = "qq_group_blacklist.json";
/// `scope=all` 在账本里的群号位置。
const ALL_GROUPS: &str = "*";
const MAX_HOURS: f64 = 24.0 * 365.0;
const MAX_REASON_CHARS: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    /// 到期时刻（unix 秒）；None = 永久。
    until: Option<i64>,
    reason: String,
    added_by: String,
    added_at: i64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Ledger {
    /// 键：`账号:群号:QQ号`，全局条目的群号是 `*`。
    #[serde(default)]
    entries: BTreeMap<String, Entry>,
}

fn file(paths: &GqyPaths) -> PathBuf {
    paths.state_dir.join(FILE_NAME)
}

fn key(account: &str, group: &str, user: &str) -> String {
    format!("{account}:{group}:{user}")
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn active(entry: &Entry, now: i64) -> bool {
    entry.until.is_none_or(|until| until > now)
}

fn plugin_enabled(config: &AppConfig) -> bool {
    config
        .platforms
        .qq
        .plugins
        .get(QQ_GROUP_BLACKLIST_PLUGIN_ID)
        .and_then(|instance| instance.enabled)
        .unwrap_or(true)
}

/// 这个人在这个群里是否被拉黑（本群条目或全局条目，未过期）。插件关掉时一律 false。
pub(crate) fn is_blacklisted(
    config: &AppConfig,
    paths: &GqyPaths,
    account: &str,
    group: &str,
    user: &str,
) -> bool {
    if !plugin_enabled(config) {
        return false;
    }
    let now = now();
    json_ledger::read(&file(paths), |ledger: &Ledger| {
        [group, ALL_GROUPS].iter().any(|group| {
            ledger
                .entries
                .get(&key(account, group, user))
                .is_some_and(|entry| active(entry, now))
        })
    })
    .unwrap_or(false)
}

pub(super) struct GroupBlacklistPlugin;

impl GroupBlacklistPlugin {
    pub(super) fn new() -> Self {
        Self
    }
}

impl PlatformPlugin for GroupBlacklistPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: QQ_GROUP_BLACKLIST_PLUGIN_ID,
            priority: 290,
            default_enabled: true,
        }
    }

    fn register_tools(
        &self,
        registry: &mut ToolRegistry,
        context: Arc<PlatformTurnContext>,
    ) -> Result<()> {
        if context.conversation.platform != "onebot" || !context.is_admin {
            return Ok(());
        }
        registry.register(
            ToolSpec::new(
                "qq_group_blacklist",
                "Make yourself ignore someone in QQ groups, for when an administrator tells you to stop responding to a person who keeps harassing or baiting you. action=add ignores them: they can still talk, but nothing they say will trigger a reply from you. action=remove undoes it. action=list shows who is ignored. By default it applies to the current group; scope=all applies to every group. hours makes it temporary; omit it for permanent. Administrators and yourself cannot be added.",
                json!({
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["add", "remove", "list"] },
                        "user_id": { "type": "string", "description": "The person's QQ number. Required by add and remove." },
                        "scope": { "type": "string", "enum": ["group", "all"], "description": "Defaults to group." },
                        "group_id": { "type": "string", "description": "The group's number. Needed only for scope=group outside that group's chat." },
                        "hours": { "type": "number", "description": "Duration for add. Omit for permanent." },
                        "reason": { "type": "string", "description": "Short note for the record." }
                    },
                    "required": ["action"],
                    "additionalProperties": false
                }),
                move |args| {
                    let context = context.clone();
                    async move { execute(&context, &args) }
                },
            )
            .writes()
            .with_display_name("QQ群黑名单"),
        );
        Ok(())
    }
}

fn arg<'a>(args: &'a Value, name: &str) -> Option<&'a str> {
    args.get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn target_group(context: &PlatformTurnContext, args: &Value) -> Result<String> {
    if arg(args, "scope") == Some("all") {
        return Ok(ALL_GROUPS.to_string());
    }
    if let Some(group) = arg(args, "group_id") {
        group
            .parse::<i64>()
            .context("group_id must be a QQ group number")?;
        return Ok(group.to_string());
    }
    if context.conversation.kind == ConversationKind::Group {
        return Ok(context.conversation.conversation_id.clone());
    }
    bail!("group_id is required outside a group chat, or use scope=all")
}

fn protected(context: &PlatformTurnContext, user: &str) -> bool {
    let account = &context.conversation.account_id;
    user == account
        || user
            .parse::<i64>()
            .is_ok_and(|id| context.config.platforms.qq.admin_users.contains(&id))
        || has_dynamic_access(
            &context.state_store,
            account,
            AccessPermission::Administrator,
            user,
        )
}

fn execute(context: &PlatformTurnContext, args: &Value) -> Result<String> {
    let account = context.conversation.account_id.clone();
    let path = file(&context.paths);
    match arg(args, "action") {
        Some("list") => {
            let now = now();
            let prefix = format!("{account}:");
            let entries = json_ledger::read(&path, |ledger: &Ledger| {
                ledger
                    .entries
                    .iter()
                    .filter(|(key, entry)| key.starts_with(&prefix) && active(entry, now))
                    .map(|(key, entry)| {
                        let mut parts = key.splitn(3, ':').skip(1);
                        let group = parts.next().unwrap_or_default();
                        json!({
                            "group": if group == ALL_GROUPS { "all" } else { group },
                            "user_id": parts.next().unwrap_or_default(),
                            "until": entry.until,
                            "reason": entry.reason,
                        })
                    })
                    .collect::<Vec<_>>()
            })?;
            Ok(json!({ "ok": true, "entries": entries }).to_string())
        }
        Some(action @ ("add" | "remove")) => {
            let user = arg(args, "user_id")
                .context("user_id is required")?
                .to_string();
            user.parse::<i64>().context("user_id must be a QQ number")?;
            let group = target_group(context, args)?;
            let key = key(&account, &group, &user);
            if action == "remove" {
                let removed = json_ledger::update(&path, |ledger: &mut Ledger| {
                    let removed = ledger.entries.remove(&key).is_some();
                    (removed, removed)
                })?;
                return Ok(json!({ "ok": removed, "removed": removed }).to_string());
            }
            if protected(context, &user) {
                return Ok(json!({ "ok": false, "reason": "Administrators and yourself cannot be ignored." }).to_string());
            }
            let hours = args
                .get("hours")
                .and_then(Value::as_f64)
                .filter(|hours| *hours > 0.0);
            if hours.is_some_and(|hours| hours > MAX_HOURS) {
                bail!("hours must be at most {MAX_HOURS}");
            }
            let now = now();
            let entry = Entry {
                until: hours.map(|hours| now + (hours * 3600.0) as i64),
                reason: arg(args, "reason")
                    .unwrap_or_default()
                    .chars()
                    .take(MAX_REASON_CHARS)
                    .collect(),
                added_by: context.sender_id.clone(),
                added_at: now,
            };
            let until = entry.until;
            json_ledger::update(&path, |ledger: &mut Ledger| {
                ledger.entries.insert(key, entry);
                ((), true)
            })?;
            Ok(json!({ "ok": true, "user_id": user, "scope": if group == ALL_GROUPS { "all" } else { "group" }, "until": until }).to_string())
        }
        _ => bail!("action must be add, remove or list"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platforms::tests::shared::test_turn_context;

    #[test]
    fn group_and_global_entries_expire_and_admins_are_protected() {
        let (_temp, mut context, _adapter) = test_turn_context(false);
        context.conversation.kind = ConversationKind::Group;
        context.conversation.conversation_id = "500".to_string();
        let account = context.conversation.account_id.clone();
        let config = context.config.clone();
        let paths = context.paths.clone();

        let added = execute(&context, &json!({ "action": "add", "user_id": "777" })).unwrap();
        assert!(added.contains("\"ok\":true"), "{added}");
        assert!(is_blacklisted(&config, &paths, &account, "500", "777"));
        assert!(
            !is_blacklisted(&config, &paths, &account, "501", "777"),
            "group scope only"
        );

        execute(
            &context,
            &json!({ "action": "add", "user_id": "888", "scope": "all" }),
        )
        .unwrap();
        assert!(is_blacklisted(&config, &paths, &account, "501", "888"));

        // 到期即失效：直接写一条过期条目。
        json_ledger::update(&file(&paths), |ledger: &mut Ledger| {
            ledger.entries.insert(
                key(&account, "500", "999"),
                Entry {
                    until: Some(now() - 1),
                    reason: String::new(),
                    added_by: String::new(),
                    added_at: 0,
                },
            );
            ((), true)
        })
        .unwrap();
        assert!(!is_blacklisted(&config, &paths, &account, "500", "999"));

        execute(&context, &json!({ "action": "remove", "user_id": "777" })).unwrap();
        assert!(!is_blacklisted(&config, &paths, &account, "500", "777"));

        context.config.platforms.qq.admin_users = vec![123];
        let refused = execute(&context, &json!({ "action": "add", "user_id": "123" })).unwrap();
        assert!(refused.contains("\"ok\":false"), "{refused}");
    }
}
