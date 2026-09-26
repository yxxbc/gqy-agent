//! 假连接器走真 WebSocket：握手、鉴权、ack、发送回执、重连顶掉旧连接。
//! 不跑真回合（要模型），回合那一段由 inbound.rs 的单元测试和手机实测覆盖。

use super::adapter::ConnectorAdapter;
use super::connector_ws;
use crate::config::{ConnectorContact, ConnectorPlatformConfig};
use crate::platforms::tests::shared::test_paths;
use crate::platforms::{OutboundMessage, OutboundOrigin, PlatformAdapter};
use crate::runtime::DaemonState;
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Client = WebSocketStream<MaybeTlsStream<TcpStream>>;

const TOKEN: &str = "test-token";

async fn serve(root: &std::path::Path) -> (DaemonState, String) {
    let state = DaemonState::for_test(test_paths(root), 0).unwrap();
    state
        .manager
        .lock()
        .unwrap()
        .config
        .platforms
        .connectors
        .insert(
            "imessage".into(),
            ConnectorPlatformConfig {
                enabled: true,
                token: TOKEN.into(),
                contacts: vec![ConnectorContact {
                    name: "me".into(),
                    handles: vec!["+8613800000000".into()],
                    owner: true,
                }],
                ..Default::default()
            },
        );
    let app = Router::new()
        .route("/api/connector/ws", get(connector_ws))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (
        state,
        format!("ws://{address}/api/connector/ws?platform=imessage"),
    )
}

async fn connect(url: &str, token: &str) -> Result<Client, tokio_tungstenite::tungstenite::Error> {
    let mut request = url.into_client_request().unwrap();
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    tokio_tungstenite::connect_async(request)
        .await
        .map(|(client, _)| client)
}

async fn send(client: &mut Client, frame: Value) {
    client
        .send(Message::Text(frame.to_string().into()))
        .await
        .unwrap();
}

/// 下一个非 ping 的帧。
async fn next(client: &mut Client) -> Value {
    loop {
        let message = tokio::time::timeout(Duration::from_secs(5), client.next())
            .await
            .expect("frame within 5s")
            .expect("stream open")
            .unwrap();
        if let Message::Text(text) = message {
            let frame: Value = serde_json::from_str(&text).unwrap();
            if frame["type"] != "ping" {
                return frame;
            }
        }
    }
}

fn hello(capabilities: Value) -> Value {
    json!({
        "type": "hello",
        "protocol": "gqy-connector/1",
        "platform": "imessage",
        "display_name": "iMessage",
        "connector": { "name": "test", "version": "1" },
        "capabilities": capabilities,
    })
}

async fn handshake(url: &str) -> Client {
    let mut client = connect(url, TOKEN).await.unwrap();
    send(&mut client, hello(json!({ "image_out": true }))).await;
    let welcome = next(&mut client).await;
    assert_eq!(welcome["type"], "welcome");
    assert_eq!(welcome["protocol"], "gqy-connector/1");
    client
}

async fn wait_connected(state: &DaemonState) -> super::registry::ConnectorHandle {
    for _ in 0..50 {
        if let Some(handle) = state.platforms.connectors.handle("imessage", "") {
            return handle;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("connector never registered");
}

#[tokio::test]
async fn wrong_or_missing_token_is_rejected_before_upgrade() {
    let temp = tempfile::tempdir().unwrap();
    let (_state, url) = serve(temp.path()).await;
    assert!(connect(&url, "nope").await.is_err());
    let unknown = url.replace("platform=imessage", "platform=telegram");
    assert!(connect(&unknown, TOKEN).await.is_err());
}

#[tokio::test]
async fn hello_for_another_platform_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let (_state, url) = serve(temp.path()).await;
    let mut client = connect(&url, TOKEN).await.unwrap();
    let mut frame = hello(json!({}));
    frame["platform"] = json!("telegram");
    send(&mut client, frame).await;
    let error = next(&mut client).await;
    assert_eq!(error["type"], "error");
    assert_eq!(error["code"], "bad_hello");
}

#[tokio::test]
async fn events_from_strangers_and_reactions_are_acked_without_a_turn() {
    let temp = tempfile::tempdir().unwrap();
    let (state, url) = serve(temp.path()).await;
    let mut client = handshake(&url).await;
    send(
        &mut client,
        json!({
            "type": "event", "id": "100", "kind": "message",
            "conversation": { "kind": "private", "id": "+8613900000000" },
            "sender": { "id": "+8613900000000" },
            "text": "hi"
        }),
    )
    .await;
    assert_eq!(
        next(&mut client).await,
        json!({ "type": "ack", "id": "100" })
    );

    send(
        &mut client,
        json!({
            "type": "event", "id": "101", "kind": "reaction",
            "conversation": { "kind": "private", "id": "+8613800000000" },
            "sender": { "id": "+8613800000000" },
            "reaction": "❤️",
            "target": { "text": "晚安", "from_me": true }
        }),
    )
    .await;
    assert_eq!(
        next(&mut client).await,
        json!({ "type": "ack", "id": "101" })
    );
    assert_eq!(
        state
            .platforms
            .connectors
            .take_notes("imessage::private:me"),
        vec!["[reacted ❤️ to your message: \"晚安\"]".to_string()]
    );

    // 连接器没收到 ack 又重发：直接回 ack，不再处理。
    send(
        &mut client,
        json!({
            "type": "event", "id": "101", "kind": "reaction",
            "conversation": { "kind": "private", "id": "+8613800000000" },
            "sender": { "id": "+8613800000000" },
            "reaction": "❤️"
        }),
    )
    .await;
    assert_eq!(
        next(&mut client).await,
        json!({ "type": "ack", "id": "101" })
    );
    assert!(state
        .platforms
        .connectors
        .take_notes("imessage::private:me")
        .is_empty());
}

#[tokio::test]
async fn adapter_sends_bubbles_and_waits_for_results() {
    let temp = tempfile::tempdir().unwrap();
    let (state, url) = serve(temp.path()).await;
    let mut client = handshake(&url).await;
    let handle = wait_connected(&state).await;
    let settings = ConnectorPlatformConfig {
        bubble_pause_seconds: 0.0,
        ..Default::default()
    };
    let adapter = ConnectorAdapter::new(handle, "+8613800000000".into(), &settings);
    let sending = tokio::spawn(async move {
        adapter
            .send(OutboundMessage::markdown(
                OutboundOrigin::FinalReply,
                "**第一段**\n\n第二段",
            ))
            .await
    });
    for (expected, id) in [("第一段", "m1"), ("第二段", "m2")] {
        let frame = next(&mut client).await;
        assert_eq!(frame["type"], "send");
        assert_eq!(frame["to"], "+8613800000000");
        assert_eq!(frame["part"], json!({ "kind": "text", "text": expected }));
        send(
            &mut client,
            json!({ "type": "send_result", "req": frame["req"], "ok": true, "message_id": id }),
        )
        .await;
    }
    let receipt = sending.await.unwrap().unwrap();
    assert_eq!(receipt.delivered_parts, 2);
    assert_eq!(receipt.message_ids, vec!["m1", "m2"]);
}

#[tokio::test]
async fn failed_send_after_a_delivered_bubble_is_partial() {
    let temp = tempfile::tempdir().unwrap();
    let (state, url) = serve(temp.path()).await;
    let mut client = handshake(&url).await;
    let handle = wait_connected(&state).await;
    let settings = ConnectorPlatformConfig {
        bubble_pause_seconds: 0.0,
        ..Default::default()
    };
    let adapter = ConnectorAdapter::new(handle, "+8613800000000".into(), &settings);
    let sending = tokio::spawn(async move {
        adapter
            .send(OutboundMessage::markdown(
                OutboundOrigin::FinalReply,
                "一\n\n二",
            ))
            .await
    });
    let first = next(&mut client).await;
    send(
        &mut client,
        json!({ "type": "send_result", "req": first["req"], "ok": true }),
    )
    .await;
    let second = next(&mut client).await;
    send(
        &mut client,
        json!({ "type": "send_result", "req": second["req"], "ok": false, "error": "not delivered" }),
    )
    .await;
    let error = sending.await.unwrap().unwrap_err();
    let partial = error
        .downcast_ref::<crate::platforms::PartialSendError>()
        .expect("partial send");
    assert_eq!(partial.receipt().delivered_parts, 1);
}

#[tokio::test]
async fn audio_is_refused_when_the_connector_cannot_send_it() {
    let temp = tempfile::tempdir().unwrap();
    let (state, url) = serve(temp.path()).await;
    let _client = handshake(&url).await;
    let handle = wait_connected(&state).await;
    let path = temp.path().join("voice.wav");
    std::fs::write(&path, b"RIFF").unwrap();
    let adapter = ConnectorAdapter::new(handle, "x".into(), &ConnectorPlatformConfig::default());
    let error = adapter
        .send(OutboundMessage::segments(
            OutboundOrigin::Tool,
            vec![crate::platforms::OutboundSegment::AudioPath {
                path,
                transcript: String::new(),
            }],
        ))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("cannot send voice messages"));
}

#[tokio::test]
async fn reconnect_closes_the_old_connection() {
    let temp = tempfile::tempdir().unwrap();
    let (state, url) = serve(temp.path()).await;
    let mut first = handshake(&url).await;
    let old = wait_connected(&state).await;
    let _second = handshake(&url).await;
    let closed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match first.next().await {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
                Some(Ok(_)) => continue,
            }
        }
    })
    .await;
    assert!(closed.is_ok(), "old connection should be closed");
    let current = wait_connected(&state).await;
    assert_ne!(current.id, old.id);
}

mod commands {
    use super::super::commands::{execute, load_prefs, parse, CommandScope};
    use super::super::legacy;
    use crate::platforms::tests::shared::test_paths;
    use crate::platforms::{resolve_platform_session, ConversationKind, PlatformConversation};
    use crate::runtime::DaemonState;

    fn conversation() -> PlatformConversation {
        PlatformConversation {
            platform: "imessage".into(),
            account_id: String::new(),
            kind: ConversationKind::Private,
            conversation_id: "me".into(),
        }
    }

    fn run(state: &DaemonState, text: &str) -> String {
        let conversation = conversation();
        let command = parse("/", text, false).expect("command");
        execute(
            &CommandScope {
                state,
                conversation: &conversation,
                persona: "default",
            },
            &command,
        )
    }

    fn current_name(state: &DaemonState) -> String {
        let session =
            resolve_platform_session(state, &conversation(), "default", None, "imessage-me", None)
                .unwrap();
        state
            .state_store
            .session_record(&session)
            .unwrap()
            .unwrap()
            .name
    }

    #[test]
    fn new_and_topic_rebind_the_conversation() {
        let temp = tempfile::tempdir().unwrap();
        let state = DaemonState::for_test(test_paths(temp.path()), 0).unwrap();
        assert_eq!(current_name(&state), "imessage-me");
        assert!(run(&state, "/new").ends_with('2'));
        assert_eq!(current_name(&state), "imessage-me-2");
        assert!(run(&state, "/topics").contains("▶ 2."));
        run(&state, "/topic 1");
        assert_eq!(current_name(&state), "imessage-me");
        assert!(run(&state, "/topic 9").contains('9'));
        assert_eq!(current_name(&state), "imessage-me");
    }

    #[test]
    fn pause_and_resume_persist() {
        let temp = tempfile::tempdir().unwrap();
        let state = DaemonState::for_test(test_paths(temp.path()), 0).unwrap();
        run(&state, "/pause");
        assert!(load_prefs(&state, &conversation()).paused);
        run(&state, "/resume");
        assert!(!load_prefs(&state, &conversation()).paused);
    }

    #[test]
    fn legacy_bridge_topic_and_pause_are_migrated_once() {
        let temp = tempfile::tempdir().unwrap();
        let state = DaemonState::for_test(test_paths(temp.path()), 0).unwrap();
        for name in ["imessage-me", "imessage-me-3"] {
            state
                .state_store
                .create_session("default", name, "user", None)
                .unwrap();
        }
        std::fs::create_dir_all(&state.paths.state_dir).unwrap();
        std::fs::write(
            state.paths.state_dir.join("imessage-contacts.json"),
            r#"{"me": {"topic": 3, "paused": true, "model": "no-such-model"}}"#,
        )
        .unwrap();
        let prefs = load_prefs(&state, &conversation());
        legacy::migrate(&state, &conversation(), "default", &prefs);
        assert_eq!(current_name(&state), "imessage-me-3");
        let prefs = load_prefs(&state, &conversation());
        assert!(prefs.paused && prefs.legacy_migrated);

        // 搬过一次就不再读旧文件：用户切回话题 1 后不会被旧文件拽回去。
        run(&state, "/topic 1");
        legacy::migrate(&state, &conversation(), "default", &prefs);
        assert_eq!(current_name(&state), "imessage-me");
    }
}
