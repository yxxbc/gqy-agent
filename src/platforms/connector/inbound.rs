//! 连接器报上来的一条事件 → 一个平台回合。
//!
//! 第一版只接私聊：名单里的联系人发来消息，按联系人找（或建）会话，跑回合，
//! 回复经连接器发回去。与 QQ 私聊同一套规矩：
//!
//! - 到达顺序位在最前面拿，后到的消息不会先排上。
//! - 她还在回上一条时又来一条：工具在跑就排进当前回合，只是在写回复就取代它
//!   （`active_turn_update_mode`，08-29 「先装瞎再答题」取证）。
//! - 点按回应不开回合，攒着跟下一条消息一起给她看。
//!
//! 平台指令（/new /topics /model …）不开回合，见 commands.rs；暂停中的对话
//! 消息直接丢（和旧桥接一样不补回）。

use super::adapter::ConnectorAdapter;
use super::protocol::{Event, EventKind, Quote, MAX_ATTACHMENT_BYTES};
use super::registry::ConnectorHandle;
use super::{commands, legacy};
use crate::config::{ConnectorContact, PlatformSessionLimits, PromptAudience};
use crate::i18n::text as t;
use crate::ipc::ImageAttachment;
use crate::platforms::plugins::real_context::safe_prompt_field;
use crate::platforms::*;
use crate::runtime::{enqueue_turn_update, TurnUpdateRequest};
use crate::state::QueuedPromptAttachment;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;

/// 一条消息最多带几张图进模型。
const MAX_INBOUND_IMAGES: usize = 4;

/// 私聊串行：同一个人的消息一条一条回。
const SESSION_LIMITS: PlatformSessionLimits = PlatformSessionLimits {
    running: 1,
    queued: 16,
};

/// 引用 / 点按回应里的原文最多带多少字。
const SNIPPET_CHARS: usize = 60;

pub(super) async fn handle_event(
    state: &DaemonState,
    handle: &ConnectorHandle,
    event: Event,
    ingress_order: i64,
) {
    let (app_config, settings) = {
        let manager = state.manager.lock().unwrap();
        match manager.config.platforms.connectors.get(&handle.platform) {
            Some(settings) if settings.enabled => (manager.config.clone(), settings.clone()),
            _ => return,
        }
    };
    if event.conversation.kind != "private" {
        tracing::debug!(target: "gqy::platform", platform = %handle.platform, kind = %event.conversation.kind, "{}", t("connector group events are not supported yet", "连接器群聊事件暂不支持"));
        return;
    }
    let Some(contact) = settings.contact_for_handle(&event.sender.id).cloned() else {
        tracing::info!(target: "gqy::platform", platform = %handle.platform, "{}", t("connector message from someone outside the contact list ignored", "名单外联系人的连接器消息已忽略"));
        return;
    };
    let conversation = PlatformConversation {
        platform: handle.platform.clone(),
        account_id: handle.account.clone(),
        kind: ConversationKind::Private,
        conversation_id: contact.name.clone(),
    };
    let scope = conversation.scope_key();
    if event.kind == EventKind::Reaction {
        if !event.reaction.trim().is_empty() {
            state.platforms.connectors.push_note(
                &scope,
                reaction_note(&event.reaction, event.target.as_ref()),
            );
        }
        return;
    }
    let persona = app_config.active_persona_scope();
    let prefs = commands::load_prefs(state, &conversation);
    if !prefs.legacy_migrated {
        legacy::migrate(state, &conversation, &persona, &prefs);
    }
    let adapter = Arc::new(ConnectorAdapter::new(
        handle.clone(),
        event.sender.id.clone(),
        &settings,
    ));
    if let Some(command) = commands::parse(
        &app_config.platforms.command_prefix,
        &event.text,
        !event.attachments.is_empty(),
    ) {
        let reply = commands::execute(
            &commands::CommandScope {
                state,
                conversation: &conversation,
                persona: &persona,
            },
            &command,
        );
        if let Err(error) = adapter
            .send(OutboundMessage::text(OutboundOrigin::Command, reply))
            .await
        {
            tracing::warn!(target: "gqy::platform", error = %error, "{}", t("connector command reply failed", "连接器指令回复失败"));
        }
        return;
    }
    if commands::load_prefs(state, &conversation).paused {
        tracing::debug!(target: "gqy::platform", platform = %handle.platform, "{}", t("connector conversation is paused; message ignored", "连接器对话已暂停，消息已忽略"));
        return;
    }
    let Some(order_slot) = state.platforms.turn_order.enter(
        &scope,
        ingress_order,
        SESSION_LIMITS.running.saturating_add(SESSION_LIMITS.queued),
    ) else {
        tracing::debug!(target: "gqy::platform", platform = %handle.platform, "{}", t("connector message discarded: the conversation queue is full", "连接器消息已丢弃：当前会话等待队列已满"));
        return;
    };

    let notes = state.platforms.connectors.take_notes(&scope);
    let input = build_input(&event, notes);
    let inbound_event = inbound_event(&conversation, &contact, &event, &input.text, ingress_order);
    let plugins = match state.platforms.plugins() {
        Ok(plugins) => plugins,
        Err(error) => {
            tracing::warn!(target: "gqy::platform", error = %error, "{}", t("connector platform runtime initialization failed", "连接器平台运行时初始化失败"));
            return;
        }
    };
    let context = Arc::new(
        PlatformTurnContext::new(
            conversation.clone(),
            contact.name.clone(),
            contact.name.clone(),
            contact.owner,
            app_config,
            state.paths.clone(),
            state.state_store.clone(),
            adapter,
            plugins,
        )
        .with_config_manager(state.manager.clone())
        .with_inbound_event(inbound_event.clone()),
    );
    let session_id = match resolve_platform_session(
        state,
        &conversation,
        &persona,
        None,
        &session_name(&handle.platform, &contact.name),
        None,
    ) {
        Ok(session_id) => session_id,
        Err(error) => {
            tracing::warn!(target: "gqy::platform", error = %error, "{}", t("resolving the connector session failed", "解析连接器会话失败"));
            let _ = context
                .send_bypass_plugins(OutboundMessage::text(
                    OutboundOrigin::Command,
                    t(
                        "Something went wrong while opening this conversation.",
                        "打开当前会话时出错了。",
                    ),
                ))
                .await;
            return;
        }
    };

    if let Some((run_id, turn_id, followup)) =
        platform_update_target(state, &session_id, &conversation, &contact.name)
    {
        let tool_executing =
            reserve_tool_followup(state, &session_id, &conversation, &contact.name).is_some();
        let mode = active_turn_update_mode(false, tool_executing);
        let _ingress_reservation = followup.try_reserve();
        let _enqueue_order = followup.lock_enqueue().await;
        let result = enqueue_turn_update(
            state,
            TurnUpdateRequest {
                run_id,
                turn_id,
                session_id: Some(session_id.clone()),
                audience: PromptAudience::External,
                content: input.content(),
                display_content: input.content(),
                attachments: input
                    .images
                    .iter()
                    .map(|(mime, data)| QueuedPromptAttachment::Binary {
                        mime: mime.clone(),
                        data_base64: BASE64.encode(data),
                    })
                    .collect(),
                uploaded_attachment_ids: Vec::new(),
                mode,
            },
        );
        match result {
            Ok(_) => followup.context.accept_followup(&inbound_event),
            Err(error) => {
                tracing::warn!(target: "gqy::platform", %session_id, error = %error, "{}", t("connector follow-up could not be queued", "连接器后续消息无法入队"))
            }
        }
        return;
    }

    let ticket =
        state
            .platforms
            .session_turn_ticket_in_order(&session_id, SESSION_LIMITS, order_slot);
    let Ok(lease) = ticket.acquire().await else {
        return;
    };
    if !lease.is_valid() {
        context.after_turn_aborted().await;
        return;
    }
    tracing::info!(
        target: "gqy::platform",
        platform = %handle.platform,
        %session_id,
        text_chars = input.text.chars().count(),
        images = input.images.len(),
        "{}",
        t("connector message accepted", "连接器消息已接受")
    );

    let content = input.content();
    let images = input
        .images
        .into_iter()
        .map(|(mime, data)| Some(ImageAttachment::Binary { mime, data }))
        .collect();
    let prepared = context.prepare_turn(content).await;
    let profile = TurnProfile {
        active_persona: Some(context.config.prompt.active_persona.clone()),
        // 不按平台路由模型：会话自己钉的模型（/model）优先，否则用全局默认。
        text_models: None,
        multimodal_models: None,
        system_context: {
            let mut system = vec![chat_policy(&handle.display_name)];
            system.extend(prepared.system_context);
            system
        },
        turn_system_context: prepared.turn_system_context,
        memory_content: Some(prepared.memory_content),
        context_images: prepared.context_images,
        context_files: prepared.context_files.into_boxed_slice(),
        platform: Some(context.clone()),
        image_cache_namespace: Some(handle.platform.clone()),
        image_source_label: Some(handle.display_name.clone()),
        memory_write_enabled: settings.memory_write_enabled,
        suppress_session_history: false,
        group_context: None,
        followup: None,
    };
    let dispatch = run_platform_turn(state, session_id, prepared.content, images, profile).await;
    if !lease.is_valid() {
        context.after_turn_aborted().await;
        return;
    }
    match dispatch {
        Ok(dispatch) => match deliver_dispatch(state, &context, dispatch).await {
            Ok(true) => {
                tracing::info!(target: "gqy::platform", platform = %handle.platform, "{}", t("connector reply delivered", "连接器回复已投递"))
            }
            Ok(false) => {}
            Err(error) => {
                tracing::warn!(target: "gqy::platform", error = %error, "{}", t("connector reply delivery failed", "连接器回复投递失败"));
                context.after_turn_aborted().await;
            }
        },
        Err(error) => {
            tracing::warn!(target: "gqy::platform", error = %error, "{}", t("connector message handling failed", "连接器消息处理失败"));
            context.after_turn_aborted().await;
            let _ = context
                .send_bypass_plugins(OutboundMessage::text(
                    OutboundOrigin::Command,
                    format!(
                        "{}{}",
                        t("Something went wrong: ", "出错了："),
                        crate::runtime::safe_error_message(&error)
                    ),
                ))
                .await;
        }
    }
}

/// 会话名：`<平台>-<联系人>`，第一个话题沿用旧桥接起的名字（`imessage-<联系人>`），
/// 历史直接接上。之后的话题由 /new 另起（`-2`、`-3`……）。
pub(crate) fn session_name(platform: &str, contact: &str) -> String {
    format!("{platform}-{contact}")
}

/// 会话级常量（不随消息变），放 system 侧，不破前缀缓存。
fn chat_policy(display_name: &str) -> String {
    format!(
        "<connector-chat platform=\"{}\">This is a private chat in a phone messaging app. \
Write plain text only. Markdown shows up as raw symbols. \
A blank line splits your reply into separate message bubbles. \
Keep related sentences in the same bubble. Most replies need one to three bubbles. \
Fewer bubbles never means saying less. \
Keep it conversational and go long only when asked for detail. \
Attached images are photos sent in this chat. \
Bracketed lines such as [replying to …] or [reacted ❤️ to …] come from the chat app, not from the person's typing.</connector-chat>",
        safe_prompt_field(display_name)
    )
}

struct TurnInput {
    /// 模型看到的正文（点按回应说明、引用行、消息正文、附件占位）。
    text: String,
    images: Vec<(String, Vec<u8>)>,
}

impl TurnInput {
    fn content(&self) -> String {
        if self.text.trim().is_empty() && !self.images.is_empty() {
            "[image]".to_string()
        } else {
            self.text.clone()
        }
    }
}

fn build_input(event: &Event, notes: Vec<String>) -> TurnInput {
    let mut lines = notes;
    if let Some(quote) = event.reply_to.as_ref() {
        lines.push(format!(
            "[replying to {} message: \"{}\"]",
            owner_phrase(quote),
            snippet(&quote.text)
        ));
    }
    let text = event.text.trim();
    if !text.is_empty() {
        lines.push(text.to_string());
    }
    let mut images = Vec::new();
    for attachment in &event.attachments {
        let name = safe_prompt_field(attachment.name.trim());
        // 语音和文件第一版只给占位（不转写、不下载），连接器也不必传内容。
        match attachment.kind.as_str() {
            "audio" => {
                lines.push("[voice message]".into());
                continue;
            }
            "image" => {}
            _ => {
                lines.push(format!("[attachment: {name}]"));
                continue;
            }
        }
        if !attachment.error.trim().is_empty() || attachment.data.is_empty() {
            lines.push(format!("[attachment unavailable: {name}]"));
            continue;
        }
        let bytes = match BASE64.decode(attachment.data.as_bytes()) {
            Ok(bytes) if bytes.len() <= MAX_ATTACHMENT_BYTES => bytes,
            _ => {
                lines.push(format!("[attachment unavailable: {name}]"));
                continue;
            }
        };
        if images.len() >= MAX_INBOUND_IMAGES {
            lines.push("[image not shown: too many images in one message]".into());
            continue;
        }
        let mime = sniff_image_mime(&bytes);
        if mime == "application/octet-stream" {
            lines.push(format!("[attachment unavailable: {name}]"));
        } else {
            images.push((mime.to_string(), bytes));
        }
    }
    TurnInput {
        text: lines.join("\n"),
        images,
    }
}

fn reaction_note(reaction: &str, target: Option<&Quote>) -> String {
    let reaction = safe_prompt_field(reaction.trim());
    match target {
        Some(quote) => format!(
            "[reacted {reaction} to {} message: \"{}\"]",
            owner_phrase(quote),
            snippet(&quote.text)
        ),
        None => format!("[reacted {reaction} to a message]"),
    }
}

fn owner_phrase(quote: &Quote) -> &'static str {
    if quote.from_me {
        "your"
    } else {
        "their own"
    }
}

fn snippet(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return "[image]".into();
    }
    let mut cut: String = text.chars().take(SNIPPET_CHARS).collect();
    if text.chars().count() > SNIPPET_CHARS {
        cut.push('…');
    }
    safe_prompt_field(&cut)
}

fn inbound_event(
    conversation: &PlatformConversation,
    contact: &ConnectorContact,
    event: &Event,
    text: &str,
    ingress_order: i64,
) -> PlatformInboundEvent {
    PlatformInboundEvent {
        kind: PlatformInboundEventKind::Message,
        conversation: conversation.clone(),
        conversation_display_name: None,
        message_id: event.id.clone(),
        sender_id: contact.name.clone(),
        sender_display_name: contact.name.clone(),
        operator_id: None,
        timestamp: event.timestamp,
        received_at: std::time::Instant::now(),
        message_position: None,
        ingress_order: Some(ingress_order),
        text: text.to_string(),
        reply_to_message_id: event
            .reply_to
            .as_ref()
            .map(|quote| quote.id.clone())
            .filter(|id| !id.is_empty()),
        replied_message: None,
        mentioned_user_ids: Vec::new(),
        mentioned_users: Vec::new(),
        mentioned_bot: false,
        media: Vec::new(),
        notice_sub_type: None,
        duration_seconds: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platforms::connector::protocol::{Attachment, EventConversation, EventSender};

    fn message(text: &str) -> Event {
        Event {
            id: "1".into(),
            kind: EventKind::Message,
            conversation: EventConversation {
                kind: "private".into(),
                id: "+8613800000000".into(),
            },
            sender: EventSender {
                id: "+8613800000000".into(),
                name: String::new(),
            },
            text: text.into(),
            reply_to: None,
            reaction: String::new(),
            target: None,
            attachments: Vec::new(),
            timestamp: 0,
        }
    }

    #[test]
    fn notes_and_quote_come_before_the_text() {
        let mut event = message("好呀");
        event.reply_to = Some(Quote {
            id: "9".into(),
            text: "明天去看电影吗".into(),
            from_me: true,
        });
        let input = build_input(
            &event,
            vec!["[reacted ❤️ to your message: \"晚安\"]".into()],
        );
        assert_eq!(
            input.text,
            "[reacted ❤️ to your message: \"晚安\"]\n[replying to your message: \"明天去看电影吗\"]\n好呀"
        );
    }

    #[test]
    fn image_only_message_becomes_an_image_marker() {
        let png = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0];
        let mut event = message("");
        event.attachments.push(Attachment {
            kind: "image".into(),
            name: "a.png".into(),
            mime: "image/png".into(),
            data: BASE64.encode(png),
            error: String::new(),
        });
        let input = build_input(&event, Vec::new());
        assert_eq!(input.images.len(), 1);
        assert_eq!(input.content(), "[image]");
    }

    #[test]
    fn broken_attachments_are_named_not_hidden() {
        let mut event = message("看");
        event.attachments.push(Attachment {
            kind: "image".into(),
            name: "b.heic".into(),
            error: "conversion failed".into(),
            ..Default::default()
        });
        let input = build_input(&event, Vec::new());
        assert!(input.images.is_empty());
        assert!(input.text.ends_with("[attachment unavailable: b.heic]"));
    }

    #[test]
    fn voice_and_files_need_no_content() {
        let mut event = message("");
        for (kind, name) in [("audio", "Audio Message.caf"), ("file", "报告.pdf")] {
            event.attachments.push(Attachment {
                kind: kind.into(),
                name: name.into(),
                ..Default::default()
            });
        }
        let input = build_input(&event, Vec::new());
        assert_eq!(input.text, "[voice message]\n[attachment: 报告.pdf]");
    }

    #[test]
    fn reaction_note_quotes_the_target() {
        let note = reaction_note(
            "👍",
            Some(&Quote {
                id: String::new(),
                text: "我到了".into(),
                from_me: false,
            }),
        );
        assert_eq!(note, "[reacted 👍 to their own message: \"我到了\"]");
    }

    #[test]
    fn session_name_matches_the_old_bridge() {
        assert_eq!(session_name("imessage", "me"), "imessage-me");
    }
}
