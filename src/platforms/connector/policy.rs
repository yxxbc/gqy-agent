//! 连接器平台的策略：全部从 `platforms.connectors.<平台>` 取。
//!
//! 身份是联系人名（`sender_id` = 配置里的 `name`）：同一个人的多个账号合成一个
//! 身份，记忆与权限都跟着人走，不跟着手机号走。

use crate::config::ConnectorPlatformConfig;
use crate::platforms::{ConversationKind, PlatformPolicy};

impl PlatformPolicy for ConnectorPlatformConfig {
    fn plugin_enabled(&self, _id: &str) -> Option<bool> {
        None
    }

    fn is_owner(&self, sender_id: &str) -> bool {
        self.contact_named(sender_id)
            .is_some_and(|contact| contact.owner)
    }

    fn private_whitelisted(&self, sender_id: &str) -> bool {
        self.contact_named(sender_id).is_some()
    }

    fn admin_host_tools(&self) -> bool {
        self.owner_host_tools
    }

    fn allow_non_admin_host_tools(&self) -> bool {
        false
    }

    fn intermediate_messages(&self, _kind: ConversationKind) -> bool {
        false
    }
}
