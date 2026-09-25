//! QQ（OneBot v11 反向 WebSocket，NapCat）的平台驱动：包住 `QqListenerManager`，
//! 行为与接入驱动层之前一字不差。

use super::*;
use crate::platforms::{PlatformDriver, PreparedPlatform};
use futures_util::future::BoxFuture;

pub(crate) struct OneBotDriver;

impl PlatformDriver for OneBotDriver {
    fn id(&self) -> &'static str {
        crate::platforms::access_control::ONEBOT_PLATFORM
    }

    fn display_name(&self) -> &'static str {
        "Tencent QQ"
    }

    fn prepare<'a>(
        &'a self,
        state: &'a DaemonState,
        current: Option<&'a AppConfig>,
        next: &'a AppConfig,
    ) -> BoxFuture<'a, Result<Box<dyn PreparedPlatform>>> {
        Box::pin(async move {
            let prepared = state
                .platforms
                .qq_listener
                .prepare(
                    state,
                    current.map(|config| &config.platforms.qq),
                    &next.platforms.qq,
                )
                .await?;
            Ok(Box::new(prepared) as Box<dyn PreparedPlatform>)
        })
    }

    fn shutdown<'a>(&'a self, state: &'a DaemonState) -> BoxFuture<'a, ()> {
        Box::pin(state.platforms.qq_listener.shutdown(state))
    }
}

impl PreparedPlatform for PreparedQqListener {
    fn commit(self: Box<Self>) {
        (*self).commit();
    }
}
