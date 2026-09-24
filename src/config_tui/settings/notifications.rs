//! 通知：桌面通知总开关、回复完成通知、后台任务写回终端。

use super::spec::setting;
use crate::config_tui::*;

pub(super) fn fields(config: &AppConfig) -> BoundFields {
    let form = BoundFields::default();
    let form = setting!(
        form,
        config,
        "Desktop notifications",
        "桌面通知",
        notifications.enabled
    );
    let form = setting!(
        form,
        config,
        "Notify when a reply finishes",
        "回复完成时通知",
        notifications.on_turn_complete
    );
    setting!(
        form,
        config,
        "Write background job results back to the terminal",
        "后台任务写回终端",
        notifications.job_writeback_to_terminal
    )
}
