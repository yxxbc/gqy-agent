//! 增删发言人：只在「房间空闲且还没有任何消息」时允许（用户 09-27 定的规则）。
//!
//! 发言顺序、水位和后台会话都跟着成员表走，对话一开始就不能再动——历史发言
//! 人改掉之后，已排队的轮次和水位会指向不存在的人。两道闸：
//!
//! 1. 前置闸（内存里的发言驱动 + `room_messages` 里偷看一条）：把拒绝原因说准，
//!    也省下白建又删掉的后台会话；
//! 2. `replace_room_participants_if_empty`（落库，同一事务内）：权威判定——一条
//!    消息都没有，且成员表与请求里的 `expected_participant_ids` 逐字一致。
//!
//! 成员校验复用建房那一套（`resolve_participants`）。

use super::*;

#[derive(Deserialize)]
pub(in crate::web) struct UpdateRoomParticipantsRequest {
    expected_participant_ids: Vec<String>,
    participants: Vec<CreateRoomParticipant>,
}

fn requested_participant_id(
    request: &CreateRoomParticipant,
) -> std::result::Result<&str, ApiError> {
    match request.kind.as_str() {
        "persona" => Ok(PERSONA_PARTICIPANT_ID),
        "relay" if !request.provider_id.trim().is_empty() => Ok(&request.provider_id),
        "relay" => Err(bad_request("中转线缺少供应商")),
        _ => Err(bad_request("未知的参与者类型")),
    }
}

fn discard_new_backing_sessions(store: &StateStore, session_ids: &[String]) {
    for session_id in session_ids {
        if let Err(error) = store.delete_session(session_id) {
            tracing::warn!(%error, session_id, "discarding uncommitted room member session failed");
        }
    }
}

pub(in crate::web) async fn update_room_participants_http(
    State(state): State<DaemonState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
    Json(request): Json<UpdateRoomParticipantsRequest>,
) -> std::result::Result<Response, ApiError> {
    require_mutation(&headers, &state)?;
    require_local_web_session(&state, &headers, &room_id)?;
    let identity = require_identity(&headers, &state)?;
    if request.participants.is_empty() {
        return Err(bad_request("至少保留一位发言人"));
    }
    if request.participants.len() > MAX_PARTICIPANTS {
        return Err(bad_request("参与者太多了，最多 6 位"));
    }

    let store = state.stores.for_session(&room_id);
    let current = store
        .room_participants(&room_id)
        .map_err(ApiError::internal)?;
    if current.is_empty() {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "聊天室不存在"));
    }
    if !room_idle(&room_id) {
        return Err(conflict("聊天室正在说话，等这一轮结束再改发言人"));
    }
    // 前置闸：省下白建又删掉的后台会话，并把拒绝原因说准。权威判定在下单那次
    // `replace_room_participants_if_empty` 里（同一事务里再数一次消息）。
    if !store
        .room_messages_tail(&room_id, 1)
        .map_err(ApiError::internal)?
        .is_empty()
    {
        return Err(conflict("聊天室已经说过话了，发言人名单不能再改"));
    }
    let current_ids: Vec<String> = current
        .iter()
        .map(|participant| participant.participant_id.clone())
        .collect();
    if current_ids != request.expected_participant_ids {
        return Err(conflict("房间成员已变化，请刷新后重试"));
    }

    let current_by_id: HashMap<&str, &RoomParticipant> = current
        .iter()
        .map(|participant| (participant.participant_id.as_str(), participant))
        .collect();
    let mut requested_ids = std::collections::HashSet::new();
    let mut new_requests = Vec::new();
    for participant in &request.participants {
        let participant_id = requested_participant_id(participant)?;
        if !requested_ids.insert(participant_id.to_string()) {
            return Err(bad_request("同一位参与者只能拉进来一次"));
        }
        if let Some(existing) = current_by_id.get(participant_id) {
            if existing.kind != participant.kind {
                return Err(bad_request("参与者类型不匹配"));
            }
        } else {
            new_requests.push((*participant).clone());
        }
    }

    let config = state.manager.lock().unwrap().config.clone();
    let resolved_new = if new_requests.is_empty() {
        Vec::new()
    } else {
        resolve_participants(&config, &state.paths, &new_requests)?
    };
    let mut new_by_id: HashMap<String, RoomParticipant> = resolved_new
        .into_iter()
        .map(|participant| (participant.participant_id.clone(), participant))
        .collect();
    let owner = identity.owner_key().to_string();
    let persona = if identity.admin {
        active_persona_scope(&state)
    } else {
        member_session_persona(&state, &owner)
    };
    let mut created_backing_ids = Vec::new();
    let mut participants = Vec::with_capacity(request.participants.len());
    for (index, requested) in request.participants.iter().enumerate() {
        let participant_id = requested_participant_id(requested)?;
        let mut participant = if let Some(existing) = current_by_id.get(participant_id) {
            (*existing).clone()
        } else {
            let mut participant = new_by_id
                .remove(participant_id)
                .ok_or_else(|| bad_request("参与者列表无效，请刷新后重试"))?;
            let backing = match store.create_session_for_owner(
                &persona,
                &participant.label,
                ROOM_MEMBER_SESSION_KIND,
                Some(&room_id),
                &owner,
            ) {
                Ok(backing) => backing,
                Err(error) => {
                    discard_new_backing_sessions(&store, &created_backing_ids);
                    return Err(ApiError::internal(error));
                }
            };
            created_backing_ids.push(backing.session_id.clone());
            if !participant.provider_id.is_empty() {
                if let Err(error) = store.set_session_model_override(
                    &backing.session_id,
                    Some(&[crate::config::ActiveProviderModelConfig {
                        provider_id: participant.provider_id.clone(),
                        model: participant.model.clone(),
                    }]),
                ) {
                    discard_new_backing_sessions(&store, &created_backing_ids);
                    return Err(ApiError::internal(error));
                }
            }
            participant.backing_session_id = backing.session_id;
            participant
        };
        participant.sort = index as i64 + 1;
        participants.push(participant);
    }

    let replaced =
        match store.replace_room_participants_if_empty(&room_id, &current_ids, &participants) {
            Ok(replaced) => replaced,
            Err(error) => {
                discard_new_backing_sessions(&store, &created_backing_ids);
                return Err(ApiError::internal(error));
            }
        };
    if !replaced {
        discard_new_backing_sessions(&store, &created_backing_ids);
        return Err(conflict("聊天室已开始对话或成员已变化，请刷新后重试"));
    }

    for session_id in &created_backing_ids {
        state.stores.note_session_owner(session_id, &owner);
    }
    let retained_ids: std::collections::HashSet<&str> = participants
        .iter()
        .map(|participant| participant.participant_id.as_str())
        .collect();
    for participant in &current {
        if !retained_ids.contains(participant.participant_id.as_str()) {
            crate::llm::forget_relay_sessions(&participant.backing_session_id);
            if let Err(error) = store.delete_session(&participant.backing_session_id) {
                tracing::warn!(%error, participant = %participant.participant_id, "deleting removed room member session failed");
            }
        }
    }

    let view = room_view_json(&store, &room_id)
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "聊天室不存在"))?;
    state.events.publish(
        "room.participants",
        json!({
            "session_id": room_id,
            "room_id": room_id,
            "participants": view["participants"],
        }),
    );
    Ok(Json(json!({ "room": view })).into_response())
}
