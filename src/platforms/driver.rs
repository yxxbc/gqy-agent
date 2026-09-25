//! 平台驱动：每个接入的平台（QQ、以后的 iMessage、Telegram……）一个，负责连接的
//! 起停与配置热重载。daemon 启动、`gqy reload`、WebUI 保存配置、退出时遍历全部驱动，
//! 不再点名 QQ。
//!
//! 重载是两段式：先对新配置 `prepare`（可能失败，比如端口被占——失败就整次保存作废，
//! 旧配置和旧连接原样不动），等配置真正换上之后再 `commit`。
//!
//! 回合、会话、投递那一侧是平台中立的（`common/`），驱动只管「连接从哪来」。

use super::*;
use futures_util::future::BoxFuture;

pub(crate) trait PlatformDriver: Send + Sync {
    /// 与会话绑定、`PlatformConversation::platform` 一致的平台标识（QQ 是 `onebot`）。
    fn id(&self) -> &'static str;

    /// 给人看的名字，报错里用（「Tencent QQ listener configuration failed: …」）。
    fn display_name(&self) -> &'static str;

    fn prepare<'a>(
        &'a self,
        state: &'a DaemonState,
        current: Option<&'a AppConfig>,
        next: &'a AppConfig,
    ) -> BoxFuture<'a, Result<Box<dyn PreparedPlatform>>>;

    fn shutdown<'a>(&'a self, state: &'a DaemonState) -> BoxFuture<'a, ()>;
}

/// 已准备好、等配置换上后提交的连接变更。
pub(crate) trait PreparedPlatform: Send {
    fn commit(self: Box<Self>);
}

/// 一次配置变更里所有平台的准备结果：全部准备成功才会拿到它，再一起提交。
pub(crate) struct PreparedPlatforms(Vec<Box<dyn PreparedPlatform>>);

impl PreparedPlatforms {
    pub(crate) fn commit(self) {
        for prepared in self.0 {
            prepared.commit();
        }
    }
}

/// 某个平台准备失败：哪个平台、为什么。调用方按原来的口径拼报错。
pub(crate) struct PlatformPrepareError {
    pub(crate) display_name: &'static str,
    pub(crate) error: anyhow::Error,
}

impl PlatformRuntime {
    pub(crate) fn drivers(&self) -> &[Arc<dyn PlatformDriver>] {
        &self.drivers
    }

    pub(crate) async fn prepare_all(
        &self,
        state: &DaemonState,
        current: Option<&AppConfig>,
        next: &AppConfig,
    ) -> std::result::Result<PreparedPlatforms, PlatformPrepareError> {
        let mut prepared = Vec::with_capacity(self.drivers.len());
        for driver in self.drivers.iter() {
            match driver.prepare(state, current, next).await {
                Ok(item) => prepared.push(item),
                Err(error) => {
                    return Err(PlatformPrepareError {
                        display_name: driver.display_name(),
                        error,
                    })
                }
            }
        }
        Ok(PreparedPlatforms(prepared))
    }

    pub(crate) async fn shutdown_all(&self, state: &DaemonState) {
        for driver in self.drivers.iter() {
            driver.shutdown(state).await;
        }
    }
}
