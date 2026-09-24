//! 工具：开关、轮数、加载模式、子代理与超时、命令黑名单、沙盒，外加 Skills 与 MCP 总开关。

use super::spec::setting;
use crate::config_tui::*;

pub(super) fn fields(config: &AppConfig) -> BoundFields {
    let form = BoundFields::default();
    let form = setting!(form, config, "Enable tools", "工具启用", tools.enabled);
    let form = setting!(
        form,
        config,
        "Maximum tool rounds",
        "工具最大轮数",
        tools.max_rounds
    );
    let form = form.with_path(
        "tools.loading_mode",
        Field::new(
            t("Tool loading mode", "工具加载模式"),
            normalize_tools_loading_mode(&config.tools.loading_mode),
        )
        .choices(&["full", "stub"]),
        |config, value| {
            config.tools.loading_mode = normalize_tools_loading_mode(value);
            Ok(())
        },
    );
    let form = setting!(
        form,
        config,
        "Remember loaded tools",
        "记住已加载工具",
        tools.persist_loaded_tools
    );
    let form = setting!(
        form,
        config,
        "Concurrent subagents",
        "子代理并发数",
        tools.subagent_concurrency
    );
    let form = setting!(
        form,
        config,
        "Tool timeout fallback (seconds, 0 = off)",
        "工具兜底超时(秒,0=关闭)",
        tools.default_timeout_secs
    );
    let form = setting!(
        form,
        config,
        "Denied command substrings",
        "命令拒绝子串",
        tools.command_deny
    );
    let form = setting!(
        form,
        config,
        "Extra sandbox writable paths",
        "沙盒额外可写",
        tools.sandbox.writable
    );
    let form = setting!(
        form,
        config,
        "Extra sandbox read-only paths",
        "沙盒额外只读",
        tools.sandbox.readable
    );
    let form = setting!(form, config, "Enable skills", "Skills 启用", skills.enabled);
    let form = setting!(
        form,
        config,
        "Allow command execution",
        "允许执行命令",
        skills.allow_command_execution
    );
    setting!(form, config, "Enable MCP", "MCP 启用", mcp.enabled)
}
