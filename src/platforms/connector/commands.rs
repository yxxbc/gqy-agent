//! 连接器平台的指令：`/help` `/new` `/topics` `/topic N` `/model [N|名字|default]`
//! `/pause` `/resume`。手机上的快捷指令也发这些。
//!
//! 指令不进模型、不开回合，daemon 直接回一句话。整条消息就是指令才算（带
//! 附件的不算），前缀跟 `platforms.command_prefix`（缺省 `/`）。
//!
//! 话题 = 同一个联系人名下的多个会话：第一个叫 `<平台>-<联系人>`，之后是
//! `<平台>-<联系人>-2`、`-3`……（旧桥接起的名字，历史直接接上）。换话题就是把
//! 这个对话的会话绑定改指到另一个会话。模型用会话级覆盖，跟着话题走。

use super::inbound::session_name;
use crate::config::{resolve_provider_model_argument, ActiveProviderModelConfig};
use crate::i18n::text as t;
use crate::platforms::*;
use crate::state::{PlatformPluginScopeKey, SessionOverview};
use serde::{Deserialize, Serialize};

const COMMANDS: &[&str] = &["help", "new", "topics", "topic", "model", "pause", "resume"];
const PREFS_PLUGIN: &str = "connector";
const PREFS_KEY: &str = "prefs";

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Command {
    pub(super) name: &'static str,
    pub(super) argument: Option<String>,
}

pub(super) fn parse(prefix: &str, text: &str, has_attachments: bool) -> Option<Command> {
    if has_attachments {
        return None;
    }
    let rest = text.trim().strip_prefix(prefix)?;
    let mut words = rest.splitn(2, char::is_whitespace);
    let word = words.next()?.to_ascii_lowercase();
    let name = COMMANDS.iter().find(|name| **name == word)?;
    let argument = words
        .next()
        .map(str::trim)
        .filter(|argument| !argument.is_empty())
        .map(str::to_string);
    Some(Command { name, argument })
}

/// 每个对话存在插件表里的偏好。
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub(super) struct ContactPrefs {
    pub(super) paused: bool,
    /// 旧桥接的偏好（当前话题、模型、暂停）已经搬过来了。
    pub(super) legacy_migrated: bool,
}

fn prefs_scope(conversation: &PlatformConversation) -> PlatformPluginScopeKey {
    PlatformPluginScopeKey {
        plugin_id: PREFS_PLUGIN.to_string(),
        platform: conversation.platform.clone(),
        account_id: conversation.account_id.clone(),
        conversation_kind: conversation.kind.as_str().to_string(),
        conversation_id: conversation.conversation_id.clone(),
    }
}

pub(super) fn load_prefs(state: &DaemonState, conversation: &PlatformConversation) -> ContactPrefs {
    state
        .state_store
        .plugin_get_json(&prefs_scope(conversation), PREFS_KEY)
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub(super) fn update_prefs(
    state: &DaemonState,
    conversation: &PlatformConversation,
    update: impl FnOnce(&mut ContactPrefs),
) -> Result<()> {
    state.state_store.plugin_update_json(
        &prefs_scope(conversation),
        PREFS_KEY,
        |current: Option<ContactPrefs>| {
            let mut prefs = current.unwrap_or_default();
            update(&mut prefs);
            Ok(Some(prefs))
        },
    )?;
    Ok(())
}

pub(super) fn binding_key(
    conversation: &PlatformConversation,
    persona: &str,
) -> crate::state::PlatformSessionBindingKey {
    crate::state::PlatformSessionBindingKey {
        platform: conversation.platform.clone(),
        account_id: conversation.account_id.clone(),
        conversation_kind: conversation.kind.as_str().to_string(),
        conversation_id: conversation.conversation_id.clone(),
        participant_id: None,
        persona: persona.to_string(),
    }
}

pub(super) fn topic_name(base: &str, number: usize) -> String {
    if number <= 1 {
        base.to_string()
    } else {
        format!("{base}-{number}")
    }
}

fn topic_number(base: &str, name: &str) -> Option<usize> {
    if name == base {
        return Some(1);
    }
    name.strip_prefix(base)?
        .strip_prefix('-')?
        .parse::<usize>()
        .ok()
        .filter(|number| *number >= 2)
}

/// 这个联系人的全部话题，按编号排好。
fn topics(state: &DaemonState, persona: &str, base: &str) -> Result<Vec<(usize, SessionOverview)>> {
    let mut topics: Vec<(usize, SessionOverview)> = state
        .state_store
        .list_sessions(persona)?
        .into_iter()
        .filter(|overview| overview.record.kind == "user" && !overview.record.archived)
        .filter_map(|overview| Some((topic_number(base, &overview.record.name)?, overview)))
        .collect();
    topics.sort_by_key(|(number, _)| *number);
    Ok(topics)
}

pub(super) struct CommandScope<'a> {
    pub(super) state: &'a DaemonState,
    pub(super) conversation: &'a PlatformConversation,
    pub(super) persona: &'a str,
}

impl CommandScope<'_> {
    fn base(&self) -> String {
        session_name(
            &self.conversation.platform,
            &self.conversation.conversation_id,
        )
    }

    fn current_session(&self) -> Result<Arc<str>> {
        resolve_platform_session(
            self.state,
            self.conversation,
            self.persona,
            None,
            &self.base(),
            None,
        )
    }

    fn switch_to(&self, session_id: &str) -> Result<()> {
        self.state
            .state_store
            .bind_platform_session(&binding_key(self.conversation, self.persona), session_id)
    }
}

pub(super) fn execute(scope: &CommandScope<'_>, command: &Command) -> String {
    let result = match command.name {
        "help" => Ok(help(
            &scope
                .state
                .manager
                .lock()
                .unwrap()
                .config
                .platforms
                .command_prefix,
        )),
        "pause" => {
            update_prefs(scope.state, scope.conversation, |prefs| prefs.paused = true).map(|()| {
                t(
                    "Paused. Messages you send now are ignored until /resume.",
                    "已暂停。之后的消息我先不回，发 /resume 恢复。",
                )
                .to_string()
            })
        }
        "resume" => update_prefs(scope.state, scope.conversation, |prefs| {
            prefs.paused = false
        })
        .map(|()| t("Resumed.", "已恢复。").to_string()),
        "new" => new_topic(scope),
        "topics" => list_topics(scope),
        "topic" => switch_topic(scope, command.argument.as_deref()),
        "model" => model(scope, command.argument.as_deref()),
        _ => Ok(help("/")),
    };
    result.unwrap_or_else(|error| {
        tracing::warn!(target: "gqy::platform", command = command.name, error = %error, "{}", t("connector command failed", "连接器指令执行失败"));
        t("That command did not go through.", "这条指令没执行成功。").to_string()
    })
}

fn help(prefix: &str) -> String {
    if crate::i18n::is_zh() {
        format!(
            "可用指令：\n{p}new 开一个新话题\n{p}topics 看所有话题\n{p}topic N 切到第 N 个话题\n{p}model 看 / 换模型（{p}model default 恢复默认）\n{p}pause 暂停回复\n{p}resume 恢复回复",
            p = prefix
        )
    } else {
        format!(
            "Commands:\n{p}new start a new topic\n{p}topics list topics\n{p}topic N switch to topic N\n{p}model list or switch models ({p}model default resets)\n{p}pause stop replying\n{p}resume start replying again",
            p = prefix
        )
    }
}

fn new_topic(scope: &CommandScope<'_>) -> Result<String> {
    let base = scope.base();
    let current = scope.current_session()?;
    let topics = topics(scope.state, scope.persona, &base)?;
    let next = topics
        .iter()
        .map(|(number, _)| *number)
        .max()
        .unwrap_or(1)
        .saturating_add(1);
    let name = topic_name(&base, next);
    let record = scope
        .state
        .state_store
        .create_session(scope.persona, &name, "user", None)?;
    // 新话题沿用当前话题钉的模型：换话题不该悄悄换模型。
    if let Some(models) = scope.state.state_store.session_model_override(&current)? {
        scope
            .state
            .state_store
            .set_session_model_override(&record.session_id, Some(&models))?;
    }
    scope.switch_to(&record.session_id)?;
    scope.state.events.publish(
        "session.created",
        serde_json::json!({
            "session_id": record.session_id,
            "name": record.name,
            "platform": scope.conversation.platform,
            "conversation_id": scope.conversation.conversation_id,
        }),
    );
    Ok(format!("{}{next}", t("Started topic ", "已开新话题 ")))
}

fn list_topics(scope: &CommandScope<'_>) -> Result<String> {
    let base = scope.base();
    let current = scope.current_session()?;
    let topics = topics(scope.state, scope.persona, &base)?;
    if topics.is_empty() {
        return Ok(t("No topics yet.", "还没有话题。").to_string());
    }
    let now = chrono::Utc::now();
    let mut lines = vec![t("Topics:", "话题：").to_string()];
    for (number, overview) in &topics {
        let marker = if overview.record.session_id == *current {
            "▶"
        } else {
            "  "
        };
        let ago = chrono::DateTime::parse_from_rfc3339(&overview.record.updated_at)
            .map(|time| relative_time(now.signed_duration_since(time)))
            .unwrap_or_default();
        let last = overview
            .last_user_content
            .as_deref()
            .map(|content| {
                let content = content.split_whitespace().collect::<Vec<_>>().join(" ");
                let mut cut: String = content.chars().take(24).collect();
                if content.chars().count() > 24 {
                    cut.push('…');
                }
                cut
            })
            .unwrap_or_default();
        lines.push(
            format!(
                "{marker} {number}. {} · {ago} {last}",
                turns(overview.turn_count)
            )
            .trim_end()
            .to_string(),
        );
    }
    lines.push(t("Switch with /topic N", "发 /topic N 切换").to_string());
    Ok(lines.join("\n"))
}

fn turns(count: i64) -> String {
    if crate::i18n::is_zh() {
        format!("{count} 轮")
    } else {
        format!("{count} turns")
    }
}

fn relative_time(elapsed: chrono::Duration) -> String {
    let minutes = elapsed.num_minutes().max(0);
    let zh = crate::i18n::is_zh();
    match minutes {
        0 => (if zh { "刚刚" } else { "just now" }).to_string(),
        1..=59 => format!("{minutes}{}", if zh { " 分钟前" } else { "m ago" }),
        60..=1439 => format!("{}{}", minutes / 60, if zh { " 小时前" } else { "h ago" }),
        _ => format!("{}{}", minutes / 1440, if zh { " 天前" } else { "d ago" }),
    }
}

fn switch_topic(scope: &CommandScope<'_>, argument: Option<&str>) -> Result<String> {
    let Some(number) = argument.and_then(|argument| argument.parse::<usize>().ok()) else {
        return Ok(t(
            "Usage: /topic N (see /topics)",
            "用法：/topic N（先发 /topics 看编号）",
        )
        .to_string());
    };
    let base = scope.base();
    let current = scope.current_session()?;
    let target = topics(scope.state, scope.persona, &base)?
        .into_iter()
        .find(|(candidate, _)| *candidate == number);
    let Some((_, overview)) = target else {
        return Ok(format!(
            "{}{number}",
            t("No such topic: ", "没有这个话题：")
        ));
    };
    if overview.record.session_id == *current {
        return Ok(format!("{}{number}", t("Already on topic ", "已经在话题 ")));
    }
    scope.switch_to(&overview.record.session_id)?;
    Ok(format!(
        "{}{number}",
        t("Switched to topic ", "已切到话题 ")
    ))
}

fn model(scope: &CommandScope<'_>, argument: Option<&str>) -> Result<String> {
    let current = scope.current_session()?;
    let (choices, defaults) = {
        let manager = scope.state.manager.lock().unwrap();
        (
            manager.config.text_provider_model_choices(),
            manager
                .config
                .active_provider_models
                .clone()
                .unwrap_or_default(),
        )
    };
    if choices.is_empty() {
        return Ok(t("No models are configured.", "尚未配置任何模型。").to_string());
    }
    let pinned = scope.state.state_store.session_model_override(&current)?;
    let Some(argument) = argument else {
        let effective = pinned.clone().unwrap_or(defaults);
        let mut lines = vec![t("Available models:", "可用模型：").to_string()];
        for (index, choice) in choices.iter().enumerate() {
            let active = effective.first().is_some_and(|active| {
                active.provider_id == choice.provider_id && active.model == choice.model
            });
            let marker = if active {
                t(" ✅current", " ✅当前")
            } else {
                ""
            };
            lines.push(format!("{}. {}{marker}", index + 1, choice.label()));
        }
        lines.push(if pinned.is_some() {
            t(
                "This topic has its own model. /model default goes back to the default.",
                "这个话题单独指定了模型，/model default 恢复默认。",
            )
            .to_string()
        } else {
            t("Switch with /model N", "发 /model N 切换").to_string()
        });
        return Ok(lines.join("\n"));
    };
    if matches!(
        argument.to_ascii_lowercase().as_str(),
        "default" | "默认" | "reset"
    ) {
        scope
            .state
            .state_store
            .set_session_model_override(&current, None)?;
        return Ok(t(
            "This topic now uses the default model.",
            "这个话题已恢复默认模型。",
        )
        .to_string());
    }
    let selected = match resolve_provider_model_argument(&choices, argument) {
        Ok(choice) => choice.clone(),
        Err(message) => return Ok(message),
    };
    scope.state.state_store.set_session_model_override(
        &current,
        Some(&[ActiveProviderModelConfig {
            provider_id: selected.provider_id.clone(),
            model: selected.model.clone(),
        }]),
    )?;
    Ok(format!(
        "{}{}",
        t("This topic now uses: ", "这个话题已切换到："),
        selected.label()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_whole_known_commands_parse() {
        assert_eq!(
            parse("/", " /topic 2 ", false),
            Some(Command {
                name: "topic",
                argument: Some("2".into())
            })
        );
        assert_eq!(parse("/", "/NEW", false).unwrap().name, "new");
        assert!(parse("/", "/new", true).is_none());
        assert!(parse("/", "/unknown", false).is_none());
        assert!(parse("/", "new", false).is_none());
    }

    #[test]
    fn topic_numbers_follow_the_old_bridge_names() {
        assert_eq!(topic_name("imessage-me", 1), "imessage-me");
        assert_eq!(topic_name("imessage-me", 3), "imessage-me-3");
        assert_eq!(topic_number("imessage-me", "imessage-me"), Some(1));
        assert_eq!(topic_number("imessage-me", "imessage-me-3"), Some(3));
        assert_eq!(topic_number("imessage-me", "imessage-me-1"), None);
        assert_eq!(topic_number("imessage-me", "imessage-meow"), None);
        assert_eq!(topic_number("imessage-me", "imessage-me-x"), None);
    }
}
