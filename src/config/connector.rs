//! 连接器平台配置：`platforms.connectors.<平台>`。
//!
//! 连接器是 daemon 之外的小程序（iMessage 连接器、以后的 Telegram……），经通用
//! 连接器协议接进来（`docs/design/2026-09-26-connector-protocol.md`）。每个平台
//! 一小节配置，键就是连接器握手时报的平台名。加平台不改这里的结构。

use crate::config::*;

/// 连接器平台名：小写字母开头，只含小写字母、数字、`_`、`-`，最长 32 字节。
/// `onebot` 留给 QQ，不能拿来当连接器平台名。
pub(crate) fn valid_connector_platform_id(id: &str) -> bool {
    id.len() <= 32
        && id.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_-".contains(&byte))
        && id != "onebot"
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConnectorPlatformConfig {
    pub enabled: bool,
    /// 连接器握手用的口令。空 = 拒绝一切连接：本机其他进程（包括沙盒里的成员
    /// 会话）也能连回环端口，不能凭「来自本机」就放行。
    #[serde(skip_serializing_if = "String::is_empty")]
    pub token: String,
    /// 白名单联系人。名单外的人发来的消息确认收下但不回。
    pub contacts: Vec<ConnectorContact>,
    /// 主人的会话能不能用宿主工具（跑命令、读写文件）。默认关：手机丢了或账号
    /// 被盗时，别人能借聊天窗口在电脑上执行命令。
    pub owner_host_tools: bool,
    /// 一条回复最多拆成几个气泡（按段落均衡合并）。
    pub max_bubbles: usize,
    /// 气泡之间模拟打字的停顿上限（秒）。
    pub bubble_pause_seconds: f64,
    pub memory_write_enabled: bool,
}

impl Default for ConnectorPlatformConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            token: String::new(),
            contacts: Vec::new(),
            owner_host_tools: false,
            max_bubbles: 6,
            bubble_pause_seconds: 2.0,
            memory_write_enabled: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectorContact {
    /// 联系人名，也是会话绑定的对话标识。改名等于换一个对话。
    pub name: String,
    /// 同一个人的多个账号（手机号、邮箱……），合成一个对话。
    pub handles: Vec<String>,
    /// 主人本人：记忆与终端 / WebUI 共享。
    #[serde(default, skip_serializing_if = "is_false")]
    pub owner: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// 账号归一化：邮箱小写；电话去掉空格、横线、括号，11 位 1 开头的大陆手机号补
/// `+86`。连接器和配置两边写法不同也能对上。
pub(crate) fn normalize_connector_handle(handle: &str) -> String {
    let handle = handle.trim();
    if handle.contains('@') {
        return handle.to_lowercase();
    }
    let compact: String = handle
        .chars()
        .filter(|character| !matches!(character, ' ' | '-' | '(' | ')' | '\u{a0}'))
        .collect();
    if compact.len() == 11
        && compact.starts_with('1')
        && compact.bytes().all(|byte| byte.is_ascii_digit())
    {
        return format!("+86{compact}");
    }
    compact
}

impl ConnectorPlatformConfig {
    /// 发消息的账号属于哪个联系人。
    pub(crate) fn contact_for_handle(&self, handle: &str) -> Option<&ConnectorContact> {
        let handle = normalize_connector_handle(handle);
        if handle.is_empty() {
            return None;
        }
        self.contacts.iter().find(|contact| {
            contact
                .handles
                .iter()
                .any(|candidate| normalize_connector_handle(candidate) == handle)
        })
    }

    pub(crate) fn contact_named(&self, name: &str) -> Option<&ConnectorContact> {
        self.contacts.iter().find(|contact| contact.name == name)
    }

    pub(crate) fn validate(&self, platform: &str) -> Result<()> {
        if !valid_connector_platform_id(platform) {
            bail!(
                "platforms.connectors key {platform:?} must be a lowercase id (a-z, 0-9, _ or -) of at most 32 bytes, and not \"onebot\""
            );
        }
        if self.max_bubbles == 0 || self.max_bubbles > 20 {
            bail!("platforms.connectors.{platform}.max_bubbles must be between 1 and 20");
        }
        if !self.bubble_pause_seconds.is_finite()
            || !(0.0..=10.0).contains(&self.bubble_pause_seconds)
        {
            bail!("platforms.connectors.{platform}.bubble_pause_seconds must be between 0 and 10");
        }
        let mut names = std::collections::HashSet::new();
        let mut handles = std::collections::HashSet::new();
        for contact in &self.contacts {
            let name = contact.name.trim();
            if name.is_empty() || name != contact.name || name.chars().count() > 64 {
                bail!(
                    "platforms.connectors.{platform}.contacts: names must be trimmed, non-empty and at most 64 characters"
                );
            }
            if name.chars().any(char::is_control) {
                bail!("platforms.connectors.{platform}.contacts: names must not contain control characters");
            }
            if !names.insert(name) {
                bail!("platforms.connectors.{platform}.contacts: duplicate contact name {name:?}");
            }
            for handle in &contact.handles {
                let normalized = normalize_connector_handle(handle);
                if normalized.is_empty() {
                    bail!("platforms.connectors.{platform}.contacts.{name}: empty handle");
                }
                if !handles.insert(normalized) {
                    bail!(
                        "platforms.connectors.{platform}.contacts: a handle is listed under more than one contact"
                    );
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_normalize_across_spellings() {
        assert_eq!(
            normalize_connector_handle("138 0000-0000"),
            "+8613800000000"
        );
        assert_eq!(
            normalize_connector_handle("+8613800000000"),
            "+8613800000000"
        );
        assert_eq!(
            normalize_connector_handle(" Some@iCloud.com "),
            "some@icloud.com"
        );
    }

    #[test]
    fn contact_lookup_merges_handles() {
        let config = ConnectorPlatformConfig {
            contacts: vec![ConnectorContact {
                name: "me".into(),
                handles: vec!["13800000000".into(), "me@icloud.com".into()],
                owner: true,
            }],
            ..Default::default()
        };
        assert_eq!(
            config.contact_for_handle("+8613800000000").unwrap().name,
            "me"
        );
        assert_eq!(
            config.contact_for_handle("ME@icloud.com").unwrap().name,
            "me"
        );
        assert!(config.contact_for_handle("+8613900000000").is_none());
    }

    #[test]
    fn validation_rejects_shared_handles_and_bad_ids() {
        let mut config = ConnectorPlatformConfig {
            contacts: vec![
                ConnectorContact {
                    name: "a".into(),
                    handles: vec!["13800000000".into()],
                    owner: false,
                },
                ConnectorContact {
                    name: "b".into(),
                    handles: vec!["+8613800000000".into()],
                    owner: false,
                },
            ],
            ..Default::default()
        };
        assert!(config.validate("imessage").is_err());
        config.contacts.pop();
        assert!(config.validate("imessage").is_ok());
        assert!(config.validate("onebot").is_err());
        assert!(config.validate("iMessage").is_err());
    }

    #[test]
    fn default_section_is_omitted_and_token_hidden_when_empty() {
        let value = serde_json::to_value(ConnectorPlatformConfig::default()).unwrap();
        assert!(value.get("token").is_none());
    }
}
