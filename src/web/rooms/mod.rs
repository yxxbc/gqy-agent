//! 多方聊天室（WebUI）。方案稿：docs/design/2026-09-27-chat-room.md。
//!
//! 房间本身是一个普通 user 会话，所以列表、改名、排序、删除全部沿用会话那
//! 一套；它和普通会话的区别只在于 `room_participants` 里有它的行。每位参与者
//! 另有一个隐藏的后台会话（`room-member`，父会话 = 房间），回合在那里跑，
//! 发言驱动见 `driver`，提示词拼装见 `prompt`。

mod driver;
mod prompt;

pub(in crate::web) use driver::stop_room;

use crate::state::{RoomParticipant, ROOM_MEMBER_SESSION_KIND, ROOM_SPEAKER_USER};
use crate::web::*;

/// 打开房间时下发最近多少条消息。
const ROOM_MESSAGE_TAIL: usize = 400;
/// 一个房间最多几位 AI 参与者：每条消息大家轮流回，人越多一轮越长。
const MAX_PARTICIPANTS: usize = 6;
/// 单条消息上限（字符）。
const MAX_ROOM_MESSAGE_CHARS: usize = 20_000;
/// 顾清影 本人在房间里的参与者 id。
const PERSONA_PARTICIPANT_ID: &str = "gqy";

/// 以自身名义出场的中转线在房间里叫什么。
fn relay_label(provider: &crate::config::ProviderConfig) -> String {
    let label = if provider.is_claude_code() {
        "Claude"
    } else if provider.is_codex() {
        "Codex"
    } else if provider.is_antigravity() {
        "Gemini"
    } else if provider.is_cline() {
        "Cline"
    } else {
        provider.display_name.trim()
    };
    if label.is_empty() {
        provider.id.clone()
    } else {
        label.to_string()
    }
}

fn provider_models(provider: &crate::config::ProviderConfig) -> Vec<String> {
    let mut models = provider.models.clone();
    for model in &provider.custom_models {
        if !models.contains(model) {
            models.push(model.clone());
        }
    }
    models
}

/// 房间里人类一方的名字（写进参与者的提示词）。
fn host_label(identity: &WebIdentity) -> String {
    let name = identity.display_name.trim();
    if name.is_empty() {
        "User".to_string()
    } else {
        name.to_string()
    }
}

/// 可以拉进房间的参与者：顾清影 本人 + 已启用的本机 CLI 中转线。
pub(in crate::web) async fn room_candidates_http(
    State(state): State<DaemonState>,
    headers: HeaderMap,
) -> std::result::Result<Response, ApiError> {
    require_auth(&headers, &state)?;
    let config = state.manager.lock().unwrap().config.clone();
    let mut candidates = vec![json!({
        "kind": "persona",
        "participant_id": PERSONA_PARTICIPANT_ID,
        "label": crate::web::persona::persona_display_name(&config, &state.paths),
        "provider_id": "",
        "model": "",
        "models": [],
    })];
    for provider in config
        .providers
        .iter()
        .filter(|provider| provider.enabled && provider.is_builtin_cli_provider())
    {
        let models = provider_models(provider);
        let model = if provider.default_model.trim().is_empty() {
            models.first().cloned().unwrap_or_default()
        } else {
            provider.default_model.clone()
        };
        candidates.push(json!({
            "kind": "relay",
            "participant_id": provider.id,
            "label": relay_label(provider),
            "provider_id": provider.id,
            "model": model,
            "models": models,
        }));
    }
    Ok(Json(json!({ "candidates": candidates })).into_response())
}

#[derive(Deserialize)]
pub(in crate::web) struct CreateRoomParticipant {
    kind: String,
    #[serde(default)]
    provider_id: String,
    #[serde(default)]
    model: String,
}

#[derive(Deserialize)]
pub(in crate::web) struct CreateRoomRequest {
    #[serde(default)]
    name: Option<String>,
    participants: Vec<CreateRoomParticipant>,
}

fn bad_request(message: &str) -> ApiError {
    ApiError::new(StatusCode::BAD_REQUEST, message)
}

/// 把请求里的参与者核对成落库用的行（还没有后台会话 id）。
fn resolve_participants(
    config: &AppConfig,
    paths: &GqyPaths,
    requested: &[CreateRoomParticipant],
) -> std::result::Result<Vec<RoomParticipant>, ApiError> {
    if requested.is_empty() {
        return Err(bad_request("至少选一位参与者"));
    }
    if requested.len() > MAX_PARTICIPANTS {
        return Err(bad_request("参与者太多了，最多 6 位"));
    }
    let mut resolved: Vec<RoomParticipant> = Vec::new();
    for (index, request) in requested.iter().enumerate() {
        let sort = index as i64 + 1;
        let participant = match request.kind.as_str() {
            "persona" => RoomParticipant {
                participant_id: PERSONA_PARTICIPANT_ID.to_string(),
                label: crate::web::persona::persona_display_name(config, paths),
                kind: "persona".to_string(),
                provider_id: String::new(),
                model: String::new(),
                backing_session_id: String::new(),
                sort,
                watermark: 0,
                // 用户已定：只有她把聊天室记进长期记忆。
                memory: true,
            },
            "relay" => {
                let provider = config
                    .providers
                    .iter()
                    .find(|provider| provider.id == request.provider_id)
                    .filter(|provider| provider.enabled && provider.is_builtin_cli_provider())
                    .ok_or_else(|| bad_request("这条中转线没有启用"))?;
                let model = request.model.trim();
                if model.is_empty() || !provider_models(provider).iter().any(|name| name == model) {
                    return Err(bad_request("选的模型不在这条中转线的模型列表里"));
                }
                RoomParticipant {
                    participant_id: provider.id.clone(),
                    label: relay_label(provider),
                    kind: "relay".to_string(),
                    provider_id: provider.id.clone(),
                    model: model.to_string(),
                    backing_session_id: String::new(),
                    sort,
                    watermark: 0,
                    memory: false,
                }
            }
            _ => return Err(bad_request("未知的参与者类型")),
        };
        if resolved
            .iter()
            .any(|existing| existing.participant_id == participant.participant_id)
        {
            return Err(bad_request("同一位参与者只能拉进来一次"));
        }
        resolved.push(participant);
    }
    Ok(resolved)
}

pub(in crate::web) async fn create_room_http(
    State(state): State<DaemonState>,
    headers: HeaderMap,
    Json(request): Json<CreateRoomRequest>,
) -> std::result::Result<Response, ApiError> {
    require_mutation(&headers, &state)?;
    let identity = require_identity(&headers, &state)?;
    let config = state.manager.lock().unwrap().config.clone();
    let mut participants = resolve_participants(&config, &state.paths, &request.participants)?;
    let owner = identity.owner_key().to_string();
    let persona = if identity.admin {
        active_persona_scope(&state)
    } else {
        member_session_persona(&state, &owner)
    };
    let name = request
        .name
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| {
            participants
                .iter()
                .map(|participant| participant.label.as_str())
                .collect::<Vec<_>>()
                .join("、")
        });
    let store = state
        .stores
        .for_identity(&identity)
        .map_err(ApiError::internal)?;
    let room = store
        .create_session_for_owner(
            &persona,
            &name,
            crate::state::USER_SESSION_KIND,
            None,
            &owner,
        )
        .map_err(ApiError::internal)?;
    state.stores.note_session_owner(&room.session_id, &owner);
    for participant in &mut participants {
        let backing = store
            .create_session_for_owner(
                &persona,
                &participant.label,
                ROOM_MEMBER_SESSION_KIND,
                Some(&room.session_id),
                &owner,
            )
            .map_err(ApiError::internal)?;
        state.stores.note_session_owner(&backing.session_id, &owner);
        if !participant.provider_id.is_empty() {
            store
                .set_session_model_override(
                    &backing.session_id,
                    Some(&[crate::config::ActiveProviderModelConfig {
                        provider_id: participant.provider_id.clone(),
                        model: participant.model.clone(),
                    }]),
                )
                .map_err(ApiError::internal)?;
        }
        participant.backing_session_id = backing.session_id;
    }
    store
        .insert_room_participants(&room.session_id, &participants)
        .map_err(ApiError::internal)?;
    let mut session = session_record_json(&room);
    session["room"] = json!(true);
    state.events.publish(
        "session.created",
        json!({
            "session_id": room.session_id,
            "name": room.name,
            "mode": session_mode_label(&room),
            "room": true,
        }),
    );
    let view = room_view_json(&store, &room.session_id).map_err(ApiError::internal)?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "session": session, "room": view })),
    )
        .into_response())
}

/// 打开房间时的全部数据：参与者、最近的消息、此刻谁在说话。不是房间返回 None。
pub(in crate::web) fn room_view_json(store: &StateStore, room_id: &str) -> Result<Option<Value>> {
    let participants = store.room_participants(room_id)?;
    if participants.is_empty() {
        return Ok(None);
    }
    let messages = store.room_messages_tail(room_id, ROOM_MESSAGE_TAIL)?;
    Ok(Some(json!({
        "room_id": room_id,
        "participants": participants
            .iter()
            .map(|participant| json!({
                "participant_id": participant.participant_id,
                "label": participant.label,
                "kind": participant.kind,
                "provider_id": participant.provider_id,
                "model": participant.model,
            }))
            .collect::<Vec<_>>(),
        "messages": messages,
        "status": driver::room_status(room_id),
    })))
}

#[derive(Deserialize)]
pub(in crate::web) struct RoomMessageRequest {
    content: String,
}

pub(in crate::web) async fn post_room_message_http(
    State(state): State<DaemonState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
    Json(request): Json<RoomMessageRequest>,
) -> std::result::Result<Response, ApiError> {
    require_mutation(&headers, &state)?;
    require_local_web_session(&state, &headers, &room_id)?;
    let identity = require_identity(&headers, &state)?;
    let content = request.content.trim().to_string();
    if content.is_empty() {
        return Err(bad_request("消息是空的"));
    }
    if content.chars().count() > MAX_ROOM_MESSAGE_CHARS {
        return Err(bad_request("消息太长了"));
    }
    let store = state.stores.for_session(&room_id);
    if !store.is_room(&room_id).map_err(ApiError::internal)? {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "聊天室不存在"));
    }
    let message = store
        .append_room_message(&room_id, ROOM_SPEAKER_USER, "", &content, "")
        .map_err(ApiError::internal)?;
    let _ = store.touch_session(&room_id);
    state.events.publish(
        "room.message",
        json!({ "session_id": room_id, "room_id": room_id, "message": message }),
    );
    driver::enqueue_round(state.clone(), store, room_id, host_label(&identity));
    Ok((StatusCode::ACCEPTED, Json(json!({ "message": message }))).into_response())
}

pub(in crate::web) async fn stop_room_http(
    State(state): State<DaemonState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
) -> std::result::Result<Response, ApiError> {
    require_mutation(&headers, &state)?;
    require_local_web_session(&state, &headers, &room_id)?;
    stop_room(&state, &room_id);
    Ok(Json(json!({})).into_response())
}
