//! 内置 Cline(cline CLI)特殊供应商的专用编辑表单。
//!
//! 与另三条 CLI 线的表单同构、不共享字段:没有 HTTP 概念,只有启用总开关
//! 与 CLI 中转设置(落盘在 plugins.cline)。模型名不在表单里:cline 的模型 id
//! 由用户自己的 cline 供应商决定,在模型菜单里手动添加(见 models_cache::cli_catalog)。

use crate::config_tui::*;

const TOOL_SCOPES: &[&str] = &["off", "dev", "normal", "all"];

pub(in crate::config_tui) fn edit_cline_provider_form(
    stdout: &mut io::Stdout,
    provider: ProviderConfig,
    plugin: &mut crate::config::ClinePluginConfig,
) -> Result<Option<ProviderConfig>> {
    let mut fields = vec![
        Field::new(
            t("Enabled (Cline relay)", "启用(中转 Cline)"),
            provider.enabled.to_string(),
        )
        .choices(&["true", "false"]),
        Field::new(t("Display name", "显示名称"), provider.display_name.clone()),
        Field::new(
            t("cline binary (empty = PATH)", "cline 可执行文件(空=PATH)"),
            plugin.binary.clone(),
        ),
        Field::new(
            t(
                "cline provider id (empty = CLI default)",
                "cline 供应商 id(空=CLI 默认)",
            ),
            plugin.provider.clone(),
        ),
        Field::new(
            t("cline native tools scope", "cline 原生工具作用域"),
            plugin.native_tools.clone(),
        )
        .choices(TOOL_SCOPES),
        Field::new(
            t(
                "GQY tools via MCP bridge scope",
                "顾清影 工具挂给 cline 的作用域",
            ),
            plugin.gqy_tools.clone(),
        )
        .choices(TOOL_SCOPES),
        Field::new(
            t("Stream idle watchdog (seconds)", "流空闲看门狗(秒)"),
            plugin.idle_timeout_seconds.to_string(),
        ),
    ];
    loop {
        if !run_form(stdout, t(" EDIT CLINE ", " 编辑 Cline "), &mut fields)? {
            return Ok(None);
        }
        let enabled = match parse_bool_field(&fields[0].value) {
            Ok(value) => value,
            Err(error) => {
                message(stdout, &format!("{error:#}"))?;
                continue;
            }
        };
        plugin.binary = fields[2].value.trim().to_string();
        plugin.provider = fields[3].value.trim().to_string();
        plugin.native_tools = normalize_scope(&fields[4].value);
        plugin.gqy_tools = normalize_scope(&fields[5].value);
        plugin.idle_timeout_seconds = fields[6].value.trim().parse().unwrap_or(300);
        let mut updated = provider.clone();
        updated.enabled = enabled;
        let display_name = fields[1].value.trim();
        updated.display_name = if display_name.is_empty() {
            "Cline".to_string()
        } else {
            display_name.to_string()
        };
        return Ok(Some(updated));
    }
}

fn normalize_scope(value: &str) -> String {
    let value = value.trim().to_ascii_lowercase();
    if TOOL_SCOPES.contains(&value.as_str()) {
        value
    } else {
        "off".to_string()
    }
}
