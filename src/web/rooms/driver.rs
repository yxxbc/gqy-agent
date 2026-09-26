//! 房间的发言驱动：每条人类消息排一轮，参与者按顺序依次各回一次
//! （用户 09-27 定的规则，方案稿 §3）。
//!
//! 一个房间同一时间只有一位在说话：顺序发言让时间线确定，也不会出现同一
//! 后台会话并发两回合。发言中又来了新消息，就在队尾多排一轮。
//!
//! 每位参与者的回合跑在它自己的后台会话里：登记 run → 发 `StartTurn` →
//! 按 run_id 收事件（与 `run_platform_turn`、goal 驱动器同一套）。流式增量
//! 转发成 `room.delta`，收尾写进房间记录并发 `room.message`。

use super::prompt;
use crate::state::{RoomParticipant, ROOM_SPEAKER_NOTICE, ROOM_SPEAKER_PARTICIPANT};
use crate::web::*;
use std::sync::LazyLock;

/// 一位参与者一次发言的上限。CLI 冷启动、长回复都算在里面。
const ROOM_TURN_TIMEOUT: Duration = Duration::from_secs(15 * 60);

#[derive(Default)]
struct RoomRuntime {
    running: bool,
    pending_rounds: usize,
    /// (participant_id, run_id)
    speaking: Option<(String, String)>,
    stopped: bool,
}

static ROOMS: LazyLock<Mutex<HashMap<String, RoomRuntime>>> = LazyLock::new(Default::default);

pub(super) fn room_status(room_id: &str) -> Value {
    let rooms = ROOMS.lock().unwrap();
    match rooms.get(room_id) {
        Some(runtime) => json!({
            "running": runtime.running,
            "pending_rounds": runtime.pending_rounds,
            "speaking": runtime.speaking.as_ref().map(|(participant, _)| participant),
            "run_id": runtime.speaking.as_ref().map(|(_, run_id)| run_id),
        }),
        None => json!({ "running": false, "pending_rounds": 0, "speaking": null, "run_id": null }),
    }
}

fn publish_status(state: &DaemonState, room_id: &str) {
    state.events.publish(
        "room.status",
        json!({ "session_id": room_id, "room_id": room_id, "status": room_status(room_id) }),
    );
}

/// 排一轮；房间闲着就起驱动任务。
pub(super) fn enqueue_round(state: DaemonState, store: StateStore, room_id: String, host: String) {
    let spawn = {
        let mut rooms = ROOMS.lock().unwrap();
        let runtime = rooms.entry(room_id.clone()).or_default();
        runtime.pending_rounds += 1;
        if runtime.running {
            false
        } else {
            runtime.running = true;
            runtime.stopped = false;
            true
        }
    };
    publish_status(&state, &room_id);
    if spawn {
        tokio::spawn(run_rounds(state, store, room_id, host));
    }
}

/// 停下：当前发言人取消，排着的轮全部丢掉。删房间前也调它。
pub(in crate::web) fn stop_room(state: &DaemonState, room_id: &str) {
    let run_id = {
        let mut rooms = ROOMS.lock().unwrap();
        let Some(runtime) = rooms.get_mut(room_id) else {
            return;
        };
        runtime.pending_rounds = 0;
        runtime.stopped = true;
        runtime.speaking.as_ref().map(|(_, run_id)| run_id.clone())
    };
    if let Some(run_id) = run_id {
        cancel_room_run(state, &run_id);
    }
    publish_status(state, room_id);
}

fn cancel_room_run(state: &DaemonState, run_id: &str) {
    if let Some(info) = state.manager.lock().unwrap().active_runs.get(run_id) {
        let _ = info.cancel.send(true);
    }
}

fn stopped(room_id: &str) -> bool {
    ROOMS
        .lock()
        .unwrap()
        .get(room_id)
        .is_none_or(|runtime| runtime.stopped)
}

/// 取下一轮；没有了就把房间标成空闲。
fn take_round(room_id: &str) -> bool {
    let mut rooms = ROOMS.lock().unwrap();
    let Some(runtime) = rooms.get_mut(room_id) else {
        return false;
    };
    if runtime.pending_rounds > 0 && !runtime.stopped {
        runtime.pending_rounds -= 1;
        return true;
    }
    rooms.remove(room_id);
    false
}

async fn run_rounds(state: DaemonState, store: StateStore, room_id: String, host: String) {
    while take_round(&room_id) {
        let order = match store.room_participants(&room_id) {
            Ok(participants) if !participants.is_empty() => participants,
            _ => break,
        };
        let room_name = store
            .session_record(&room_id)
            .ok()
            .flatten()
            .map(|record| record.name)
            .unwrap_or_default();
        for next in &order {
            if stopped(&room_id) {
                break;
            }
            // 每位发言前重读一次：水位要用最新的（前一位刚说完的话也得算进来）。
            let participants = match store.room_participants(&room_id) {
                Ok(participants) => participants,
                Err(_) => break,
            };
            let Some(me) = participants
                .iter()
                .find(|participant| participant.participant_id == next.participant_id)
            else {
                continue;
            };
            speak(
                &state,
                &store,
                &room_id,
                &room_name,
                &participants,
                me,
                &host,
            )
            .await;
        }
        publish_status(&state, &room_id);
    }
    ROOMS.lock().unwrap().remove(&room_id);
    publish_status(&state, &room_id);
}

enum Spoken {
    Reply(String),
    Failed(String),
    Cancelled,
}

fn post_notice(
    state: &DaemonState,
    store: &StateStore,
    room_id: &str,
    participant: &str,
    text: &str,
) {
    if let Ok(message) =
        store.append_room_message(room_id, ROOM_SPEAKER_NOTICE, participant, text, "")
    {
        state.events.publish(
            "room.message",
            json!({ "session_id": room_id, "room_id": room_id, "message": message }),
        );
    }
}

async fn speak(
    state: &DaemonState,
    store: &StateStore,
    room_id: &str,
    room_name: &str,
    participants: &[RoomParticipant],
    me: &RoomParticipant,
    host: &str,
) {
    let messages = match store.room_messages_after(room_id, me.watermark) {
        Ok(messages) => messages,
        Err(error) => {
            tracing::warn!(room = %room_id, error = %error, "room messages unreadable");
            return;
        }
    };
    let Some(content) = prompt::pack_messages(&messages, participants, host, &me.participant_id)
    else {
        return;
    };
    let seen = messages
        .last()
        .map(|message| message.message_id)
        .unwrap_or(me.watermark);

    let mut profile = crate::platforms::TurnProfile {
        system_context: vec![prompt::room_policy(room_name, participants, me, host)],
        memory_write_enabled: me.memory,
        chat_only: true,
        ..Default::default()
    };
    if me.memory {
        profile.memory_content = Some(prompt::memory_lines(
            room_name,
            &messages,
            participants,
            host,
            &me.participant_id,
        ));
    }
    let mut overrides = crate::ipc::TurnOverrides {
        tool_allowlist: Some(Vec::new()),
        ..Default::default()
    };
    if !me.provider_id.is_empty() {
        overrides.models = vec![ActiveProviderModelConfig {
            provider_id: me.provider_id.clone(),
            model: me.model.clone(),
        }];
    }
    if me.kind != "persona" {
        overrides.system_prompt = Some(prompt::identity_prompt(&me.label));
    }
    if !me.memory {
        overrides.memory_writes = Some(false);
    }

    let backing: Arc<str> = me.backing_session_id.as_str().into();
    let run_id = random_id("run", 18);
    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    {
        let mut manager = state.manager.lock().unwrap();
        if manager.admin_blocks_session(&backing) {
            drop(manager);
            post_notice(
                state,
                store,
                room_id,
                &me.participant_id,
                &format!("{} 这次没回上来：顾清影 正忙着别的操作", me.label),
            );
            return;
        }
        manager.active_runs.insert(
            run_id.clone(),
            RunInfo {
                session_id: backing.clone(),
                mode: AgentMode::Normal,
                audience: PromptAudience::Owner,
                cancel: cancel_tx,
                turn_id: None,
                queue_target: None,
                supersede: Arc::new(crate::agent::TurnSupersedeSignal::default()),
                platform_followup: None,
                operation: RunOperation::Create,
                job_wake: false,
                job_wake_label: None,
                turn_origin: crate::tools::workspace::TurnOrigin::Human,
            },
        );
    }
    if let Some(runtime) = ROOMS.lock().unwrap().get_mut(room_id) {
        runtime.speaking = Some((me.participant_id.clone(), run_id.clone()));
    }
    publish_status(state, room_id);

    let after = state.events.latest_id();
    let mut subscription = state.events.subscribe_after(after);
    let sent = state.actor_tx.send(ActorCommand::StartTurn {
        run_id: run_id.clone(),
        session_id: backing,
        display_content: content.clone(),
        content,
        attachment_run_id: None,
        mode: AgentMode::Normal,
        images: Vec::new(),
        cwd: None,
        origin_tty: None,
        audience: PromptAudience::Owner,
        profile: Some(profile),
        overrides: Some(Box::new(overrides)),
        cancel: cancel_rx,
        turn_origin: Box::new(crate::tools::workspace::TurnOrigin::Human),
    });
    let spoken = if sent.is_err() {
        finish_run(&state.manager, &run_id, None);
        Spoken::Failed("顾清影 核心不可用".to_string())
    } else {
        collect(
            state,
            &mut subscription,
            after,
            room_id,
            &me.participant_id,
            &run_id,
        )
        .await
    };

    if let Some(runtime) = ROOMS.lock().unwrap().get_mut(room_id) {
        runtime.speaking = None;
    }
    match spoken {
        Spoken::Reply(text) => {
            let text = text.trim();
            if text.is_empty() {
                post_notice(
                    state,
                    store,
                    room_id,
                    &me.participant_id,
                    &format!("{} 没有说话", me.label),
                );
            } else if let Ok(message) = store.append_room_message(
                room_id,
                ROOM_SPEAKER_PARTICIPANT,
                &me.participant_id,
                text,
                &run_id,
            ) {
                state.events.publish(
                    "room.message",
                    json!({ "session_id": room_id, "room_id": room_id, "message": message }),
                );
            }
            // 水位推到本轮打包的最后一条：它自己的回复排在水位之后，下次打包时
            // 会被当作「自己的话」滤掉；发言期间人类新发的消息也不会被跳过。
            let _ = store.set_room_watermark(room_id, &me.participant_id, seen);
        }
        Spoken::Failed(message) => {
            // 不推水位：下次轮到它时这些消息再发一遍。
            post_notice(
                state,
                store,
                room_id,
                &me.participant_id,
                &format!("{} 这次没回上来：{message}", me.label),
            );
        }
        Spoken::Cancelled => {}
    }
    publish_status(state, room_id);
}

/// 收这一回合的事件，直到它结束。
async fn collect(
    state: &DaemonState,
    subscription: &mut crate::runtime::EventSubscription,
    after: u64,
    room_id: &str,
    participant: &str,
    run_id: &str,
) -> Spoken {
    let deadline = tokio::time::Instant::now() + ROOM_TURN_TIMEOUT;
    let mut text = String::new();
    let mut last_id = after;
    loop {
        let record = if let Some(record) = subscription.pending.pop_front() {
            record
        } else {
            match tokio::time::timeout_at(deadline, subscription.receiver.recv()).await {
                Err(_) => {
                    cancel_room_run(state, run_id);
                    return Spoken::Failed("等太久了，已取消".to_string());
                }
                Ok(Ok(record)) => record,
                Ok(Err(broadcast::error::RecvError::Lagged(_))) => {
                    subscription.pending = state.events.replay_after(last_id);
                    continue;
                }
                Ok(Err(broadcast::error::RecvError::Closed)) => {
                    return Spoken::Failed("顾清影 核心已停止".to_string());
                }
            }
        };
        if record.kind == "resync_required" {
            cancel_room_run(state, run_id);
            return Spoken::Failed("事件缓冲耗尽，已取消".to_string());
        }
        last_id = record.id;
        let Ok(data) = serde_json::from_str::<Value>(&record.data) else {
            continue;
        };
        if data.get("run_id").and_then(Value::as_str) != Some(run_id) {
            continue;
        }
        match record.kind.as_str() {
            "assistant.delta" => {
                if let Some(delta) = data.get("delta").and_then(Value::as_str) {
                    text.push_str(delta);
                    state.events.publish(
                        "room.delta",
                        json!({
                            "session_id": room_id,
                            "room_id": room_id,
                            "participant_id": participant,
                            "run_id": run_id,
                            "delta": delta,
                        }),
                    );
                }
            }
            // 端点重试或被新一次生成顶替：已流出的半截作废（同 run_platform_turn）。
            "reasoning.reset" | "generation.superseded" => {
                text.clear();
                state.events.publish(
                    "room.delta_reset",
                    json!({
                        "session_id": room_id,
                        "room_id": room_id,
                        "participant_id": participant,
                        "run_id": run_id,
                    }),
                );
            }
            "run.completed" => {
                let content = data
                    .get("content")
                    .and_then(Value::as_str)
                    .filter(|content| !content.trim().is_empty())
                    .map(str::to_string)
                    .unwrap_or(text);
                return Spoken::Reply(content);
            }
            "run.failed" => {
                let message = data
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("未知错误")
                    .to_string();
                return Spoken::Failed(message);
            }
            "run.cancelled" => return Spoken::Cancelled,
            _ => {}
        }
    }
}
