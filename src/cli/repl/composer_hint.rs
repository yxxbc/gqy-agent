//! 输入框为空时的淡色提示（「给 顾清影 发消息」）。
//!
//! 文案与 WebUI 同源（人格看板的 `composer_placeholder`），REPL 启动时定一次。
//! 一有输入就消失，光标仍停在行首。

use std::sync::RwLock;

use crate::cli::*;

static HINT: RwLock<String> = RwLock::new(String::new());

/// 按当前配置取一次提示文案。
pub(in crate::cli) fn refresh(config: &AppConfig, paths: &GqyPaths) {
    let text = crate::web::composer_placeholder(config, paths);
    if let Ok(mut hint) = HINT.write() {
        *hint = text;
    }
}

/// 空输入时接在前缀后面画的那段（已裁到 `width` 列、带样式）。开发模式不显示：
/// 那条车道说话的不是人格。
pub(in crate::cli) fn styled(mode: AgentMode, width: usize) -> Option<String> {
    if mode == AgentMode::Dev {
        return None;
    }
    let hint = HINT.read().ok()?;
    if hint.trim().is_empty() || width == 0 {
        return None;
    }
    let text = crate::render::clip_to_display_width(&hint, width);
    Some(format!("{}{text}\x1b[0m", crate::render::style::MUTED))
}
