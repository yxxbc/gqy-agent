//! 显示：界面语言、思考与工具调用的显示、开屏与底色。

use super::spec::setting;
use crate::config_tui::*;

pub(super) fn fields(config: &AppConfig) -> BoundFields {
    let language = language_choice_value(&config.display.language).unwrap_or("auto");
    let form = BoundFields::default().with_path(
        "display.language",
        Field::new(t("Interface language", "界面语言"), language.to_string())
            .choices(&["auto", "en", "zh"]),
        |config, value| {
            config.display.language = language_choice_value(value).unwrap_or("auto").to_string();
            Ok(())
        },
    );
    let form = setting!(form, config, "Show reasoning", "显示思考过程",
        display.reasoning; choices = &["summary", "full", "hidden"]);
    let form = setting!(form, config, "Show tool call details", "显示工具调用信息",
        display.tool_calls; choices = &["summary", "full", "hidden"]);
    let form = form.with_path(
        "display.command_output_lines",
        Field::new(
            t("Command output lines", "命令输出显示行数"),
            config.display.command_output_lines.to_string(),
        ),
        |config, value| {
            config.display.command_output_lines =
                value.trim().parse::<usize>()?.min(MAX_COMMAND_OUTPUT_LINES);
            Ok(())
        },
    );
    let form = setting!(
        form,
        config,
        "Readable tool names",
        "工具名可读显示",
        display.readable_tool_names
    );
    let form = setting!(
        form,
        config,
        "Show token usage in shell conversations",
        "Shell 无缝对话显示 Token 计数",
        display.show_token_usage
    );
    let form = form.with_path(
        "display.mixed_model_endpoint_display",
        Field::new(
            t(
                "Show current provider/model in Mixed mode",
                "Mixed 时显示本次供应商/模型",
            ),
            parse_mixed_endpoint_display(&config.display.mixed_model_endpoint_display),
        )
        .choices(&["off", "interactive", "all"]),
        |config, value| {
            config.display.mixed_model_endpoint_display = parse_mixed_endpoint_display(value);
            Ok(())
        },
    );
    let form = form.with_path(
        "display.repl_replay_turns",
        Field::new(
            t(
                "Turns replayed when resuming with gqy -c",
                "回到上次会话时回放的轮数",
            ),
            config.display.repl_replay_turns.to_string(),
        ),
        |config, value| {
            config.display.repl_replay_turns =
                value.trim().parse::<usize>()?.min(MAX_REPL_REPLAY_TURNS);
            Ok(())
        },
    );
    let form = setting!(form, config, "Welcome screen mascot", "开屏吉祥物",
        display.mascot; choices = &["portrait", "cat", "custom", "off"]);
    setting!(form, config, "Terminal background", "终端底色",
        display.theme; choices = &["auto", "dark", "light"])
}
