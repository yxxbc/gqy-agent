//! 上下文：溢出策略、窗口、压缩水位与回灌、工具输出外溢与剪枝。

use super::spec::setting;
use crate::config_tui::*;

pub(super) fn fields(config: &AppConfig) -> BoundFields {
    let form = BoundFields::default();
    let form = setting!(form, config, "When context reaches its limit", "上下文到达上限后",
        context.on_overflow; choices = &["compact", "pop"]);
    let form = setting!(
        form,
        config,
        "Default context window (tokens)",
        "默认上下文窗口(token)",
        context.default_context_window
    );
    let form = setting!(
        form,
        config,
        "Compaction trigger ratio",
        "压缩触发水位",
        context.trim_at_ratio
    );
    let form = setting!(
        form,
        config,
        "Forced compaction ratio",
        "强制压缩水位",
        context.compact_force_ratio
    );
    let form = setting!(
        form,
        config,
        "Trim batch ratio",
        "单次修剪比例",
        context.trim_batch_ratio
    );
    let form = setting!(
        form,
        config,
        "Tail kept after summary (tokens, empty = auto)",
        "摘要后保留尾部(token,留空=自动)",
        context.compact_tail_tokens
    );
    let form = setting!(
        form,
        config,
        "Summaries reuse the prefix cache",
        "摘要复用前缀缓存",
        context.compact_cache_reuse
    );
    let form = setting!(
        form,
        config,
        "Files restored after compaction",
        "压缩后回灌文件数",
        context.compact_restore_files
    );
    let form = setting!(
        form,
        config,
        "Restored file cap (tokens)",
        "回灌单文件上限(token)",
        context.compact_restore_file_tokens
    );
    let form = setting!(
        form,
        config,
        "Restore budget (tokens)",
        "回灌总预算(token)",
        context.compact_restore_total_tokens
    );
    let form = setting!(
        form,
        config,
        "Export folded transcript",
        "导出折叠原文",
        context.compact_transcript_export
    );
    let form = setting!(
        form,
        config,
        "Tool output spill threshold (bytes, 0 = off)",
        "工具输出外溢阈值(字节,0=关闭)",
        context.tool_output_spill_bytes
    );
    let form = setting!(
        form,
        config,
        "Tool result prune threshold (chars, 0 = off)",
        "工具结果剪枝阈值(字符,0=关闭)",
        context.tool_result_prune_chars
    );
    let form = setting!(
        form,
        config,
        "Pruned head kept (chars)",
        "剪枝保留头部(字符)",
        context.tool_result_prune_head_chars
    );
    setting!(
        form,
        config,
        "Pruned tail kept (chars)",
        "剪枝保留尾部(字符)",
        context.tool_result_prune_tail_chars
    )
}
