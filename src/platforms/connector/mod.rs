//! 通用连接器接入端：daemon 之外的连接器（iMessage、以后的 Telegram……）经
//! `gqy-connector/1` 协议接进来。平台相关的读写全在连接器里，daemon 只认协议。
//!
//! - `protocol`：帧类型。
//! - `server`：WebSocket 入口、鉴权、握手、读写循环、ack。
//! - `registry`：已连上的连接器、事件去重、攒着的点按回应。
//! - `inbound`：事件 → 回合。
//! - `adapter`：出站消息 → `send` 帧（拆气泡、打字停顿、附件）。
//! - `commands`：/new /topics /topic /model /pause /resume /help。
//! - `legacy`：旧 iMessage 桥接偏好的一次性搬家。
//! - `policy`：`platforms.connectors.<平台>` 的平台策略。
//!
//! 方案：`docs/design/2026-09-26-connector-protocol.md`。

mod adapter;
mod commands;
mod inbound;
mod legacy;
mod policy;
pub(crate) mod protocol;
mod registry;
mod server;
#[cfg(test)]
mod tests;

pub(crate) use registry::ConnectorRegistry;
pub(crate) use server::connector_ws;

use crate::config::AppConfig;
use crate::platforms::{PlatformDriver, PreparedPlatform};
use crate::runtime::DaemonState;
use anyhow::Result;
use futures_util::future::BoxFuture;

/// 连接器没有自己的监听端口（挂在 web 端口上），驱动只管配置变了以后断开
/// 不再合法的连接：平台关掉了、口令换了。连接器会带着新口令重连。
pub(crate) struct ConnectorDriver;

struct PreparedConnectors {
    registry: ConnectorRegistry,
    /// 还能留着的连接：（平台, 口令）。
    allowed: Vec<(String, String)>,
    previous: Vec<(String, String)>,
}

fn enabled_tokens(config: &AppConfig) -> Vec<(String, String)> {
    config
        .platforms
        .connectors
        .iter()
        .filter(|(_, connector)| connector.enabled)
        .map(|(platform, connector)| (platform.clone(), connector.token.clone()))
        .collect()
}

impl PlatformDriver for ConnectorDriver {
    fn id(&self) -> &'static str {
        "connector"
    }

    fn display_name(&self) -> &'static str {
        "Connector"
    }

    fn prepare<'a>(
        &'a self,
        state: &'a DaemonState,
        current: Option<&'a AppConfig>,
        next: &'a AppConfig,
    ) -> BoxFuture<'a, Result<Box<dyn PreparedPlatform>>> {
        Box::pin(async move {
            Ok(Box::new(PreparedConnectors {
                registry: state.platforms.connectors.clone(),
                allowed: enabled_tokens(next),
                previous: current.map(enabled_tokens).unwrap_or_default(),
            }) as Box<dyn PreparedPlatform>)
        })
    }

    fn shutdown<'a>(&'a self, state: &'a DaemonState) -> BoxFuture<'a, ()> {
        Box::pin(async move { state.platforms.connectors.disconnect_where(|_| true) })
    }
}

impl PreparedPlatform for PreparedConnectors {
    fn commit(self: Box<Self>) {
        if self.allowed == self.previous {
            return;
        }
        let allowed = self.allowed;
        let previous = self.previous;
        self.registry.disconnect_where(|handle| {
            let now = allowed
                .iter()
                .find(|(platform, _)| *platform == handle.platform);
            let before = previous
                .iter()
                .find(|(platform, _)| *platform == handle.platform);
            now.is_none() || now != before
        });
    }
}
