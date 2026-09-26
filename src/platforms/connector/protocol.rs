//! 通用连接器协议 `gqy-connector/1` 的帧类型。
//!
//! 帧是 JSON 文本，靠 `type` 区分。字段含义与时序见
//! `docs/design/2026-09-26-connector-protocol.md` §二；这里只管形状。
//! 未知字段一律忽略：连接器可以比 daemon 新，多报的东西不该让握手失败。

use serde::{Deserialize, Serialize};

pub(crate) const PROTOCOL: &str = "gqy-connector/1";

/// 单帧上限。附件走 base64，16 MiB 的图编码后约 22 MB，留点余量。
pub(crate) const MAX_FRAME_BYTES: usize = 32 * 1024 * 1024;

/// 单个附件解码后的上限（进出两个方向都按它）。
pub(crate) const MAX_ATTACHMENT_BYTES: usize = 16 * 1024 * 1024;

/// 连接器能做什么。缺省全是「不能」：没声明的能力 daemon 不会去用。
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub(crate) struct Capabilities {
    /// 能把对方的点按回应（❤️ 👍 …）作为 `reaction` 事件报上来。
    pub(crate) reaction_in: bool,
    /// 能替她发点按回应。
    pub(crate) reaction_out: bool,
    pub(crate) image_out: bool,
    pub(crate) audio_out: bool,
    pub(crate) file_out: bool,
    pub(crate) group: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum ClientFrame {
    Hello(Hello),
    Event(Box<Event>),
    SendResult(SendResult),
    Ping,
    Pong,
    Error {
        #[serde(default)]
        code: String,
        #[serde(default)]
        message: String,
    },
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Hello {
    pub(crate) protocol: String,
    pub(crate) platform: String,
    /// 同一平台接了多个账号时用来区分；只有一个账号就留空。
    #[serde(default)]
    pub(crate) account: String,
    #[serde(default)]
    pub(crate) connector: ConnectorInfo,
    #[serde(default)]
    pub(crate) capabilities: Capabilities,
    /// 平台给人看的名字（`iMessage`），进提示词和日志。缺省用平台名。
    #[serde(default)]
    pub(crate) display_name: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct ConnectorInfo {
    pub(crate) name: String,
    pub(crate) version: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EventKind {
    Message,
    Reaction,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Event {
    /// 连接器侧的唯一 id（iMessage 用 ROWID）。daemon 处理完回 `ack` 带回它。
    pub(crate) id: String,
    pub(crate) kind: EventKind,
    pub(crate) conversation: EventConversation,
    pub(crate) sender: EventSender,
    #[serde(default)]
    pub(crate) text: String,
    #[serde(default)]
    pub(crate) reply_to: Option<Quote>,
    /// `reaction` 事件的表情。
    #[serde(default)]
    pub(crate) reaction: String,
    /// `reaction` 事件指向的那条消息。
    #[serde(default)]
    pub(crate) target: Option<Quote>,
    #[serde(default)]
    pub(crate) attachments: Vec<Attachment>,
    /// 平台时间（Unix 秒）。
    #[serde(default)]
    pub(crate) timestamp: i64,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct EventConversation {
    /// `private` 或 `group`。
    pub(crate) kind: String,
    pub(crate) id: String,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct EventSender {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) name: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct Quote {
    pub(crate) id: String,
    pub(crate) text: String,
    /// 被引用 / 被点按的是不是她自己发的那条。
    pub(crate) from_me: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct Attachment {
    /// `image`、`audio`、`file`。
    pub(crate) kind: String,
    pub(crate) name: String,
    pub(crate) mime: String,
    /// base64；取不到时留空并在 `error` 里说原因。
    pub(crate) data: String,
    pub(crate) error: String,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct SendResult {
    pub(crate) req: u64,
    pub(crate) ok: bool,
    #[serde(default)]
    pub(crate) message_id: Option<String>,
    #[serde(default)]
    pub(crate) error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum ServerFrame<'a> {
    Welcome {
        protocol: &'static str,
        connection: u64,
        max_frame_bytes: usize,
        max_attachment_bytes: usize,
    },
    Ack {
        id: &'a str,
    },
    Send {
        req: u64,
        /// 连接器侧的收件人（iMessage 是对方的手机号 / 邮箱）。
        to: &'a str,
        part: SendPart<'a>,
    },
    Ping,
    Pong,
    Error {
        code: &'static str,
        message: String,
    },
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum SendPart<'a> {
    Text {
        text: &'a str,
    },
    Image {
        mime: &'a str,
        name: &'a str,
        data: String,
    },
    Audio {
        mime: &'a str,
        name: &'a str,
        data: String,
    },
    File {
        mime: &'a str,
        name: &'a str,
        data: String,
    },
}

impl ServerFrame<'_> {
    pub(crate) fn encode(&self) -> String {
        serde_json::to_string(self).expect("server frames always serialize")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_ignores_unknown_fields_and_defaults_capabilities() {
        let frame: ClientFrame = serde_json::from_str(
            r#"{"type":"hello","protocol":"gqy-connector/1","platform":"imessage","future":1}"#,
        )
        .unwrap();
        let ClientFrame::Hello(hello) = frame else {
            panic!("expected hello");
        };
        assert_eq!(hello.platform, "imessage");
        assert_eq!(hello.capabilities, Capabilities::default());
    }

    #[test]
    fn event_parses_with_optional_fields_missing() {
        let frame: ClientFrame = serde_json::from_str(
            r#"{"type":"event","id":"42","kind":"message",
                "conversation":{"kind":"private","id":"+8613800000000"},
                "sender":{"id":"+8613800000000"},"text":"hi"}"#,
        )
        .unwrap();
        let ClientFrame::Event(event) = frame else {
            panic!("expected event");
        };
        assert_eq!(event.kind, EventKind::Message);
        assert!(event.attachments.is_empty());
        assert!(event.reply_to.is_none());
    }

    #[test]
    fn send_frame_shape_is_stable() {
        let frame = ServerFrame::Send {
            req: 7,
            to: "a@b.c",
            part: SendPart::Text { text: "hello" },
        };
        assert_eq!(
            frame.encode(),
            r#"{"type":"send","req":7,"to":"a@b.c","part":{"kind":"text","text":"hello"}}"#
        );
    }
}
