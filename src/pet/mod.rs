//! 桌面悬浮窗:桌面上的一块透明、无边框小窗,里面是她(`gqy-pet` 进程)。
//!
//! 只在 `pet` feature 下编译。可执行入口是 `src/bin/pet.rs`;主程序 `gqy`
//! 不链接 wry/tao,与语音前端(`gqy-voice`)同一套口径。
//!
//! - 窗口行为(透明、置顶、拖动、位置记忆、右键菜单)在 [`window`]。
//! - 页面、脚本、立绘与自定义协议在 [`assets`];模型目录与清单补全在 [`model`];
//!   下载来的渲染运行时在 [`vendor`]。页面里不写任何磁盘路径,全走协议。
//! - daemon 的状态订阅(她在想 / 在说话 / 闲着)在 [`ipc`]。
//! - 「上次关在哪儿」是运行时状态,写 `state_dir/pet.json`([`state`]);
//!   用户偏好(缩放、置顶、模型目录)在配置的 `display.pet` 里。
//!
//! 方案与后续步骤(口型包络、情绪表情)见 `docs/design/2026-09-28-desktop-pet.md`。

mod assets;
mod ipc;
mod model;
pub mod state;
#[cfg(test)]
mod tests;
mod vendor;
mod window;

use crate::paths::GqyPaths;
use anyhow::Result;

/// 打开悬浮窗,一直跑到窗口被关掉。
pub fn run() -> Result<()> {
    let paths = GqyPaths::new()?;
    window::run(&paths)
}
