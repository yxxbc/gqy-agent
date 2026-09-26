//! 寄信:把想说的话交成一封信。
//!
//! WebUI 专属——由 `web/turns/task.rs` 在属主网页会话里按会话追加(同 artifact
//! 与分享那两件),所以不进 `compose` 的 UNITS:别的场所没有信封可点,给了这
//! 件工具只会让她写出一段前端不会渲染的 JSON。
//!
//! 结果是一段结构化 JSON(`ok` + 称呼 / 正文 / 落款),`web/lettercard.js` 把
//! 它画成一只信封卡片,点开是浮层里展开的信纸。这里不做任何落盘:信是给她说
//! 话用的,不是文件。

use super::{ToolRegistry, ToolSpec};
use anyhow::bail;
use serde_json::{json, Value};

/// 正文上限。信是短东西,长篇该写在聊天里或文件里。
const MAX_BODY_CHARS: usize = 2000;
/// 称呼与落款各留一行。
const MAX_FIELD_CHARS: usize = 120;

pub fn register_webui(registry: &mut ToolRegistry) {
    registry.register(ToolSpec::new(
        "send_letter",
        "Send the user a letter as a sealed envelope they tap open.",
        json!({
            "type": "object",
            "properties": {
                "salutation": {
                    "type": "string",
                    "description": "How you address them. Optional, for example their name."
                },
                "body": {
                    "type": "string",
                    "description": "The letter itself. Use \\n to start a new line. Keep it short."
                },
                "signature": {
                    "type": "string",
                    "description": "The closing line, for example your name. Optional."
                }
            },
            "required": ["body"],
            "additionalProperties": false
        }),
        |args| async move { send_letter(args) },
    ));
}

fn send_letter(args: Value) -> anyhow::Result<String> {
    let body = clean(args.get("body").and_then(Value::as_str), MAX_BODY_CHARS);
    if body.is_empty() {
        bail!("send_letter needs a letter body");
    }
    let salutation = clean(
        args.get("salutation").and_then(Value::as_str),
        MAX_FIELD_CHARS,
    );
    let signature = clean(
        args.get("signature").and_then(Value::as_str),
        MAX_FIELD_CHARS,
    );
    Ok(json!({
        "ok": true,
        "salutation": salutation,
        "body": body,
        "signature": signature,
    })
    .to_string())
}

/// 收一收文本:去首尾空白、压掉控制字符(换行除外)、限长。
///
/// 正文按行读,换行要留;其余控制字符会把卡片画花,一律去掉。按字符数限长而
/// 不是字节:截断半个汉字比截断更糟。
fn clean(raw: Option<&str>, max_chars: usize) -> String {
    raw.unwrap_or_default()
        .chars()
        .filter(|ch| !ch.is_control() || *ch == '\n')
        .collect::<String>()
        .trim()
        .chars()
        .take(max_chars)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_letter_keeps_its_lines_and_drops_control_chars() {
        let out = send_letter(json!({
            "salutation": "  致你  ",
            "body": "今晚的月亮很好看。\n我把月色装进信里。\u{7}",
            "signature": "影",
        }))
        .unwrap();
        let value: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(value["ok"], json!(true));
        assert_eq!(value["salutation"], "致你");
        assert_eq!(value["body"], "今晚的月亮很好看。\n我把月色装进信里。");
        assert_eq!(value["signature"], "影");
    }

    #[test]
    fn an_empty_body_is_an_error() {
        assert!(send_letter(json!({ "body": "  \n " })).is_err());
        assert!(send_letter(json!({})).is_err());
    }

    #[test]
    fn a_letter_is_capped_by_chars_not_bytes() {
        let long = "月".repeat(MAX_BODY_CHARS + 50);
        let out = send_letter(json!({ "body": long })).unwrap();
        let value: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            value["body"].as_str().unwrap().chars().count(),
            MAX_BODY_CHARS
        );
    }
}
