//! 悬浮窗的运行时状态:上次关在哪儿。
//!
//! 位置会自己变,所以不进配置文件——写配置就得整份读改写,还会和 daemon
//! 抢同一份文件(它在对话里也会改配置)。这里单独一个 `state_dir/pet.json`,
//! 丢了也无所谓,大不了回到默认位置。

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 窗口左上角(物理像素,null = 没记过)。
///
/// 用物理像素不是逻辑像素:逻辑坐标跟屏幕的缩放系数绑,换屏幕或改缩放之后
/// 同一对数会落到别的地方。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct PetState {
    #[serde(default)]
    pub x: Option<i32>,
    #[serde(default)]
    pub y: Option<i32>,
}

impl PetState {
    /// 记过的位置,两个都在才算数(只记了一半就当没记)。
    pub fn position(&self) -> Option<(i32, i32)> {
        Some((self.x?, self.y?))
    }
}

fn state_path(state_dir: &Path) -> std::path::PathBuf {
    state_dir.join("pet.json")
}

/// 读不到、读坏了都回默认:这是个锦上添花的状态,不值得为它拦启动。
pub fn load(state_dir: &Path) -> PetState {
    std::fs::read(state_path(state_dir))
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default()
}

pub fn save(state_dir: &Path, state: &PetState) -> Result<()> {
    let path = state_path(state_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_vec_pretty(state)?)?;
    Ok(())
}
