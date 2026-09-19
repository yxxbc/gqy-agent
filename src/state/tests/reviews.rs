//! 聊后复盘的落盘：最新一行为准，空 notes 能撤掉上一版。

use super::shared::*;
use crate::state::*;

#[test]
fn latest_review_wins_and_empty_notes_clear_the_previous_one() {
    let temp = tempfile::tempdir().unwrap();
    let store = StateStore::new(&test_paths(temp.path())).unwrap();
    let session = store.session_id().to_string();
    assert_eq!(store.latest_session_review(&session).unwrap(), None);

    store.start_turn("turn_1", "帮我写报告", 999999).unwrap();
    let notes = vec!["Verify counts against the screenshot first.".to_string()];
    store
        .insert_session_review(&session, "turn_1", &notes)
        .unwrap();
    assert_eq!(
        store.latest_session_review(&session).unwrap(),
        Some(("turn_1".to_string(), notes))
    );

    store.start_turn("turn_2", "谢谢", 999999).unwrap();
    store
        .insert_session_review(&session, "turn_2", &[])
        .unwrap();
    assert_eq!(
        store.latest_session_review(&session).unwrap(),
        Some(("turn_2".to_string(), Vec::new()))
    );

    let recent = store.recent_turns_of(&session, 1).unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].turn_id, "turn_2");
}

#[test]
fn review_list_is_scoped_to_persona_and_marks_the_live_version() {
    let temp = tempfile::tempdir().unwrap();
    let store = StateStore::new(&test_paths(temp.path())).unwrap();
    let mine = store
        .create_session("gqy", "报告", "user", None)
        .unwrap()
        .session_id;
    let other = store
        .create_session("other", "别的人格", "user", None)
        .unwrap()
        .session_id;
    store
        .insert_session_review(&mine, "t1", &["old".to_string()])
        .unwrap();
    store
        .insert_session_review(&mine, "t2", &["new".to_string()])
        .unwrap();
    store
        .insert_session_review(&other, "t9", &["x".to_string()])
        .unwrap();

    let (rows, total) = store.list_session_reviews("gqy", 50, 0).unwrap();
    assert_eq!(total, 2);
    assert_eq!(rows[0].notes, vec!["new".to_string()]);
    assert!(rows[0].current);
    assert_eq!(rows[0].session_name, "报告");
    assert!(!rows[1].current);
}
