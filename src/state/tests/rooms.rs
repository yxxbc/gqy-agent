//! 多方聊天室的落盘：顺序、水位只前进、随房间级联删除。

use super::shared::*;
use crate::state::*;

fn participant(id: &str, backing: &str, sort: i64) -> RoomParticipant {
    RoomParticipant {
        participant_id: id.to_string(),
        label: id.to_string(),
        kind: "relay".to_string(),
        provider_id: "claude-code".to_string(),
        model: "sonnet".to_string(),
        backing_session_id: backing.to_string(),
        sort,
        watermark: 0,
        memory: false,
    }
}

#[test]
fn room_messages_keep_order_and_watermarks_only_move_forward() {
    let temp = tempfile::tempdir().unwrap();
    let store = StateStore::new(&test_paths(temp.path())).unwrap();
    let room = store
        .create_session("gqy", "聊天室", USER_SESSION_KIND, None)
        .unwrap();
    let room_id = room.session_id.as_str();
    assert!(!store.is_room(room_id).unwrap());

    let backing_a = store
        .create_session("gqy", "a", ROOM_MEMBER_SESSION_KIND, Some(room_id))
        .unwrap();
    let backing_b = store
        .create_session("gqy", "b", ROOM_MEMBER_SESSION_KIND, Some(room_id))
        .unwrap();
    store
        .insert_room_participants(
            room_id,
            &[
                participant("b", &backing_b.session_id, 2),
                participant("a", &backing_a.session_id, 1),
            ],
        )
        .unwrap();
    assert!(store.is_room(room_id).unwrap());
    assert!(store.room_ids().unwrap().contains(room_id));
    let order: Vec<_> = store
        .room_participants(room_id)
        .unwrap()
        .into_iter()
        .map(|participant| participant.participant_id)
        .collect();
    assert_eq!(order, ["a", "b"], "参与者要按 sort 排发言顺序");

    let first = store
        .append_room_message(room_id, ROOM_SPEAKER_USER, "", "大家好", "")
        .unwrap();
    let second = store
        .append_room_message(room_id, ROOM_SPEAKER_PARTICIPANT, "a", "你好", "run_1")
        .unwrap();
    assert!(second.message_id > first.message_id);
    let after_first: Vec<_> = store
        .room_messages_after(room_id, first.message_id)
        .unwrap()
        .into_iter()
        .map(|message| message.content)
        .collect();
    assert_eq!(after_first, ["你好"]);
    let tail: Vec<_> = store
        .room_messages_tail(room_id, 1)
        .unwrap()
        .into_iter()
        .map(|message| message.content)
        .collect();
    assert_eq!(tail, ["你好"], "tail 取最近的，仍按旧→新排");

    store
        .set_room_watermark(room_id, "a", second.message_id)
        .unwrap();
    store
        .set_room_watermark(room_id, "a", first.message_id)
        .unwrap();
    let a = store
        .room_participants(room_id)
        .unwrap()
        .into_iter()
        .find(|participant| participant.participant_id == "a")
        .unwrap();
    assert_eq!(a.watermark, second.message_id, "水位不能往回退");
}

#[test]
fn deleting_a_room_takes_participants_messages_and_backing_sessions_with_it() {
    let temp = tempfile::tempdir().unwrap();
    let store = StateStore::new(&test_paths(temp.path())).unwrap();
    let room = store
        .create_session("gqy", "聊天室", USER_SESSION_KIND, None)
        .unwrap();
    let room_id = room.session_id.clone();
    let backing = store
        .create_session("gqy", "a", ROOM_MEMBER_SESSION_KIND, Some(&room_id))
        .unwrap();
    store
        .insert_room_participants(&room_id, &[participant("a", &backing.session_id, 1)])
        .unwrap();
    store
        .append_room_message(&room_id, ROOM_SPEAKER_USER, "", "hi", "")
        .unwrap();

    store.delete_session(&room_id).unwrap();

    assert!(store.room_participants(&room_id).unwrap().is_empty());
    assert!(store.room_messages_tail(&room_id, 10).unwrap().is_empty());
    assert!(
        store.session_record(&backing.session_id).unwrap().is_none(),
        "后台会话要随房间一起删掉"
    );
}

#[test]
fn replacing_room_participants_is_limited_to_empty_unchanged_rooms() {
    let temp = tempfile::tempdir().unwrap();
    let store = StateStore::new(&test_paths(temp.path())).unwrap();
    let room = store
        .create_session("gqy", "聊天室", USER_SESSION_KIND, None)
        .unwrap();
    let backing_a = store
        .create_session("gqy", "a", ROOM_MEMBER_SESSION_KIND, Some(&room.session_id))
        .unwrap();
    let backing_b = store
        .create_session("gqy", "b", ROOM_MEMBER_SESSION_KIND, Some(&room.session_id))
        .unwrap();
    store
        .insert_room_participants(
            &room.session_id,
            &[participant("a", &backing_a.session_id, 1)],
        )
        .unwrap();

    assert!(store
        .replace_room_participants_if_empty(
            &room.session_id,
            &["a".to_string()],
            &[participant("b", &backing_b.session_id, 1)],
        )
        .unwrap());
    assert_eq!(
        store.room_participants(&room.session_id).unwrap()[0].participant_id,
        "b"
    );

    store
        .append_room_message(&room.session_id, ROOM_SPEAKER_USER, "", "开始", "")
        .unwrap();
    assert!(!store
        .replace_room_participants_if_empty(
            &room.session_id,
            &["b".to_string()],
            &[participant("a", &backing_a.session_id, 1)],
        )
        .unwrap());
    assert_eq!(
        store.room_participants(&room.session_id).unwrap()[0].participant_id,
        "b"
    );
}
