//! QQ 的平台策略：全部从 `platforms.qq` 取，与接入策略层之前逐字等价。

use crate::config::OneBotConfig;
use crate::platforms::{ConversationKind, PlatformPolicy};

fn qq_number(sender_id: &str) -> Option<i64> {
    sender_id.parse::<i64>().ok()
}

impl PlatformPolicy for OneBotConfig {
    fn plugin_enabled(&self, id: &str) -> Option<bool> {
        self.plugins.get(id).and_then(|plugin| plugin.enabled)
    }

    fn is_owner(&self, sender_id: &str) -> bool {
        qq_number(sender_id).is_some_and(|sender| self.owner_users.contains(&sender))
    }

    fn private_whitelisted(&self, sender_id: &str) -> bool {
        qq_number(sender_id).is_some_and(|sender| self.private_chats.whitelist.contains(&sender))
    }

    fn allow_non_admin_host_tools(&self) -> bool {
        self.allow_non_admin_host_tools
    }

    fn intermediate_messages(&self, kind: ConversationKind) -> bool {
        match kind {
            ConversationKind::Group => self.group_intermediate_messages,
            ConversationKind::Private => self.private_intermediate_messages,
        }
    }
}
