//! 连接器的 WebSocket 入口：`GET /api/connector/ws?platform=<平台>`。
//!
//! 鉴权在升级之前做完：平台必须在 `platforms.connectors` 里启用，并且带着那一
//! 节配置的口令（`Authorization: Bearer …`）。不认「来自本机」——沙盒里的成员
//! 会话也能连回环端口（Landlock 不管 socket），放行就等于让它冒充主人发消息。
//!
//! 升级后第一帧必须是 `hello`，10 秒内不来就断。之后 daemon 每 30 秒发一次
//! `ping`，90 秒收不到任何帧就当连接器死了。

use super::inbound::handle_event;
use super::protocol::{
    ClientFrame, Hello, ServerFrame, MAX_ATTACHMENT_BYTES, MAX_FRAME_BYTES, PROTOCOL,
};
use super::registry::{ConnectorHandle, EventAdmission};
use crate::i18n::text as t;
use crate::runtime::DaemonState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{header::AUTHORIZATION, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;

const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
const PING_INTERVAL: Duration = Duration::from_secs(30);
const IDLE_TIMEOUT: Duration = Duration::from_secs(90);

/// 到达顺序号：连接读循环里按收到的先后递增，跨连接也单调。
static INGRESS_ORDER: AtomicI64 = AtomicI64::new(0);

#[derive(Deserialize)]
pub(crate) struct ConnectQuery {
    platform: String,
}

pub(crate) async fn connector_ws(
    State(state): State<DaemonState>,
    Query(query): Query<ConnectQuery>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let token = {
        let manager = state.manager.lock().unwrap();
        match manager.config.platforms.connectors.get(&query.platform) {
            Some(connector) if connector.enabled => connector.token.clone(),
            _ => return StatusCode::NOT_FOUND.into_response(),
        }
    };
    if !token_matches(&headers, &token) {
        tracing::warn!(
            target: "gqy::platform",
            platform = %query.platform,
            reason = if token.trim().is_empty() { "no_token_configured" } else { "bad_token" },
            "{}",
            t("connector rejected", "连接器已拒绝")
        );
        return StatusCode::UNAUTHORIZED.into_response();
    }
    ws.max_message_size(MAX_FRAME_BYTES)
        .max_frame_size(MAX_FRAME_BYTES)
        .on_upgrade(move |socket| connection_loop(state, socket, query.platform))
}

/// 摘要比较，不按字节短路。配置里没写口令 = 谁都不放。
pub(crate) fn token_matches(headers: &HeaderMap, expected: &str) -> bool {
    let expected = expected.trim();
    if expected.is_empty() {
        return false;
    }
    let supplied = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim);
    supplied.is_some_and(|supplied| {
        Sha256::digest(supplied.as_bytes()) == Sha256::digest(expected.as_bytes())
    })
}

async fn connection_loop(state: DaemonState, socket: WebSocket, platform: String) {
    let (mut sink, mut stream) = socket.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<String>();
    let writer = tokio::spawn(async move {
        while let Some(text) = out_rx.recv().await {
            if sink.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
        let _ = sink.close().await;
    });

    let hello = match tokio::time::timeout(HELLO_TIMEOUT, read_hello(&mut stream)).await {
        Ok(Ok(hello)) => hello,
        Ok(Err(message)) => {
            reject(&out_tx, "bad_hello", message);
            drop(out_tx);
            let _ = writer.await;
            return;
        }
        Err(_) => {
            reject(
                &out_tx,
                "hello_timeout",
                "no hello frame within 10 seconds".into(),
            );
            drop(out_tx);
            let _ = writer.await;
            return;
        }
    };
    if let Err(message) = check_hello(&hello, &platform) {
        reject(&out_tx, "bad_hello", message);
        drop(out_tx);
        let _ = writer.await;
        return;
    }

    let registry = state.platforms.connectors.clone();
    let id = registry.next_connection_id();
    let display_name = if hello.display_name.trim().is_empty() {
        platform.clone()
    } else {
        hello.display_name.trim().to_string()
    };
    let (handle, mut shutdown) = ConnectorHandle::new(
        id,
        platform.clone(),
        hello.account.trim().to_string(),
        display_name,
        hello.connector.name.clone(),
        hello.connector.version.clone(),
        hello.capabilities.clone(),
        out_tx.clone(),
    );
    let _ = handle.send_frame(&ServerFrame::Welcome {
        protocol: PROTOCOL,
        connection: id,
        max_frame_bytes: MAX_FRAME_BYTES,
        max_attachment_bytes: MAX_ATTACHMENT_BYTES,
    });
    registry.register(handle.clone());
    tracing::info!(
        target: "gqy::platform",
        platform = %handle.platform,
        account = %handle.account,
        connector = %handle.connector_name,
        version = %handle.connector_version,
        "{}",
        t("connector connected", "连接器已连接")
    );

    let mut ping = tokio::time::interval(PING_INTERVAL);
    ping.tick().await;
    let mut last_seen = tokio::time::Instant::now();
    loop {
        tokio::select! {
            _ = shutdown.changed() => break,
            _ = ping.tick() => {
                if last_seen.elapsed() > IDLE_TIMEOUT {
                    tracing::warn!(target: "gqy::platform", platform = %handle.platform, "{}", t("connector went silent; closing", "连接器无响应，已断开"));
                    break;
                }
                if handle.send_frame(&ServerFrame::Ping).is_err() {
                    break;
                }
            }
            message = stream.next() => {
                let Some(Ok(message)) = message else { break };
                last_seen = tokio::time::Instant::now();
                let text = match message {
                    Message::Text(text) => text,
                    Message::Close(_) => break,
                    _ => continue,
                };
                match serde_json::from_str::<ClientFrame>(&text) {
                    Ok(frame) => on_frame(&state, &handle, frame),
                    Err(error) => {
                        tracing::warn!(target: "gqy::platform", platform = %handle.platform, error = %error, "{}", t("connector sent a malformed frame", "连接器发来的帧格式不对"));
                        let _ = handle.send_frame(&ServerFrame::Error {
                            code: "bad_frame",
                            message: error.to_string(),
                        });
                    }
                }
            }
        }
    }
    registry.remove(id);
    drop(handle);
    drop(out_tx);
    writer.abort();
    tracing::info!(target: "gqy::platform", %platform, "{}", t("connector disconnected", "连接器已断开"));
}

async fn read_hello(
    stream: &mut futures_util::stream::SplitStream<WebSocket>,
) -> std::result::Result<Hello, String> {
    while let Some(message) = stream.next().await {
        match message {
            Ok(Message::Text(text)) => {
                return match serde_json::from_str::<ClientFrame>(&text) {
                    Ok(ClientFrame::Hello(hello)) => Ok(hello),
                    Ok(_) => Err("the first frame must be hello".into()),
                    Err(error) => Err(format!("malformed hello: {error}")),
                };
            }
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(_) => continue,
        }
    }
    Err("the connection closed before hello".into())
}

fn check_hello(hello: &Hello, platform: &str) -> std::result::Result<(), String> {
    if hello.protocol != PROTOCOL {
        return Err(format!(
            "unsupported protocol {:?}; this daemon speaks {PROTOCOL}",
            hello.protocol
        ));
    }
    if hello.platform != platform {
        return Err(format!(
            "hello platform {:?} does not match the connect URL platform {platform:?}",
            hello.platform
        ));
    }
    Ok(())
}

fn reject(out_tx: &mpsc::UnboundedSender<String>, code: &'static str, message: String) {
    tracing::warn!(target: "gqy::platform", code, %message, "{}", t("connector handshake failed", "连接器握手失败"));
    let _ = out_tx.send(ServerFrame::Error { code, message }.encode());
}

fn on_frame(state: &DaemonState, handle: &ConnectorHandle, frame: ClientFrame) {
    match frame {
        ClientFrame::Event(event) => {
            let registry = &state.platforms.connectors;
            match registry.admit_event(&handle.platform, &event.id) {
                EventAdmission::AlreadyDone => {
                    let _ = handle.send_frame(&ServerFrame::Ack { id: &event.id });
                }
                EventAdmission::InFlight => {}
                EventAdmission::New => {
                    let order = INGRESS_ORDER.fetch_add(1, Ordering::Relaxed) + 1;
                    let state = state.clone();
                    let handle = handle.clone();
                    tokio::spawn(async move {
                        let id = event.id.clone();
                        handle_event(&state, &handle, *event, order).await;
                        state
                            .platforms
                            .connectors
                            .finish_event(&handle.platform, &id);
                        // 处理完才回 ack：daemon 中途重启，连接器会把这条再发一次。
                        // 连接已换新的话，ack 发到当前那条连接上。
                        let current = state
                            .platforms
                            .connectors
                            .handle(&handle.platform, &handle.account)
                            .unwrap_or(handle);
                        let _ = current.send_frame(&ServerFrame::Ack { id: &id });
                    });
                }
            }
        }
        ClientFrame::SendResult(result) => handle.route_result(result),
        ClientFrame::Ping => {
            let _ = handle.send_frame(&ServerFrame::Pong);
        }
        ClientFrame::Pong => {}
        ClientFrame::Hello(_) => {
            let _ = handle.send_frame(&ServerFrame::Error {
                code: "duplicate_hello",
                message: "hello was already received on this connection".into(),
            });
        }
        ClientFrame::Error { code, message } => {
            tracing::warn!(target: "gqy::platform", platform = %handle.platform, %code, %message, "{}", t("connector reported an error", "连接器报告了错误"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn empty_configured_token_rejects_everyone() {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer "));
        assert!(!token_matches(&headers, ""));
        assert!(!token_matches(&HeaderMap::new(), ""));
    }

    #[test]
    fn bearer_token_must_match() {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer secret"));
        assert!(token_matches(&headers, "secret"));
        assert!(!token_matches(&headers, "other"));
        headers.insert(AUTHORIZATION, HeaderValue::from_static("secret"));
        assert!(!token_matches(&headers, "secret"));
    }

    #[test]
    fn hello_must_match_protocol_and_platform() {
        let hello: Hello =
            serde_json::from_str(r#"{"protocol":"gqy-connector/1","platform":"imessage"}"#)
                .unwrap();
        assert!(check_hello(&hello, "imessage").is_ok());
        assert!(check_hello(&hello, "telegram").is_err());
        let old: Hello =
            serde_json::from_str(r#"{"protocol":"gqy-connector/0","platform":"imessage"}"#)
                .unwrap();
        assert!(check_hello(&old, "imessage").is_err());
    }
}
