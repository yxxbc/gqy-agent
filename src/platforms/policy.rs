//! 平台策略：回合里平台中立的代码（`common/turn_context.rs`、`turn_run.rs`）要问的
//! 那几个「按平台而定」的问题。以前这些地方直读 `config.platforms.qq`，接第二个
//! 平台就得处处加分支；现在按会话所属平台取一份策略来问。
//!
//! 身份字段（`sender_id`）是宿主从平台事件里取出来的可信值（AGENTS.md §4.1），
//! 各平台自己决定怎么解析（QQ 是数字号）。没接入的平台一律「不」：宁可少给权限。

use super::*;

pub(crate) trait PlatformPolicy: Sync {
    /// 平台插件开关：`Some` = 配置里显式写了，`None` = 用插件自己的缺省值。
    fn plugin_enabled(&self, id: &str) -> Option<bool>;

    /// 配置里写明的主人（不含动态授予的管理员）：记忆与终端 / WebUI 共享。
    fn is_owner(&self, sender_id: &str) -> bool;

    /// 配置里写明的私聊白名单（静态部分；动态授权在 access_control 里另查）。
    fn private_whitelisted(&self, sender_id: &str) -> bool;

    /// 管理员能不能用宿主工具。QQ 恒为真；连接器平台由 `owner_host_tools` 决定。
    fn admin_host_tools(&self) -> bool;

    /// 白名单里的非管理员能不能用宿主工具（跑命令、读写文件等）。
    fn allow_non_admin_host_tools(&self) -> bool;

    /// 回合中途要不要把「中间消息」（边做边说的话）发出去。
    fn intermediate_messages(&self, kind: ConversationKind) -> bool;
}

/// 没接入的平台：什么都不给。
struct NoPolicy;

impl PlatformPolicy for NoPolicy {
    fn plugin_enabled(&self, _id: &str) -> Option<bool> {
        None
    }
    fn is_owner(&self, _sender_id: &str) -> bool {
        false
    }
    fn private_whitelisted(&self, _sender_id: &str) -> bool {
        false
    }
    fn admin_host_tools(&self) -> bool {
        false
    }
    fn allow_non_admin_host_tools(&self) -> bool {
        false
    }
    fn intermediate_messages(&self, _kind: ConversationKind) -> bool {
        false
    }
}

/// 会话所属平台的策略：QQ 读 `platforms.qq`，其余按连接器平台名读 `platforms.connectors`。
pub(crate) fn policy_for<'a>(config: &'a AppConfig, platform: &str) -> &'a dyn PlatformPolicy {
    match platform {
        access_control::ONEBOT_PLATFORM => &config.platforms.qq,
        other => match config.platforms.connectors.get(other) {
            Some(connector) => connector,
            None => &NoPolicy,
        },
    }
}
