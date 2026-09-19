use crate::args::WebArgs;
use crate::paths::GqyPaths;
use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};

static RESIDENT: AtomicBool = AtomicBool::new(false);

/// 本进程是常驻 daemon 吗。单次 CLI 阅后即焚:它不能留下任何等着被下一轮
/// 领走的子进程——进程一退，留下的就是孤儿。预热那类「为下一轮准备」的优化
/// 必须先问这一句。
pub(crate) fn is_resident() -> bool {
    RESIDENT.load(Ordering::Relaxed)
}

/// Unified background host for IPC, WebUI and configured platform transports.
/// Transport-specific HTTP handlers remain in `web`; lifecycle ownership lives
/// here so future entrypoints do not acquire a second process model.
pub(crate) async fn run(paths: GqyPaths, web: WebArgs) -> Result<()> {
    RESIDENT.store(true, Ordering::Relaxed);
    crate::web::run(paths, web).await
}
