//! 缓存：请求记账日志、空闲保活、写入宽限。

use super::spec::setting;
use crate::config_tui::*;

pub(super) fn fields(config: &AppConfig) -> BoundFields {
    let form = BoundFields::default();
    let form = setting!(
        form,
        config,
        "Cache accounting log",
        "请求缓存记账日志",
        cache.request_log
    );
    let form = setting!(
        form,
        config,
        "Log retention (days)",
        "记账日志保留天数",
        cache.request_log_retention_days
    );
    let form = setting!(
        form,
        config,
        "Idle keepalive interval (seconds, 0 = off)",
        "空闲保活间隔(秒,0=关闭)",
        cache.keepalive_seconds
    );
    let form = setting!(
        form,
        config,
        "Keepalive pings per turn",
        "每轮最多保活次数",
        cache.keepalive_max_pings
    );
    setting!(
        form,
        config,
        "Cache write grace (ms)",
        "缓存写入等待(毫秒)",
        cache.write_grace_ms
    )
}
