//! 斜杠命令候选的方向键选择（`docs/plan/2026-09-23-tui-commands-settings.md` §一）。
//!
//! 规则（09-10 定）：候选开着时 ↑↓ 在候选里移动（不翻历史），Tab 把选中的
//! 命令填进输入框，Enter 直接执行选中的命令，Esc 关掉候选。
//!
//! 状态挂在「当时那一串输入」上：`anchor` 记着选择是对哪串输入做的，输入一变
//! （打字、删字、填入）选择和「已关掉」自然作废，不用在每个改输入的地方挂钩子。
//! 全屏与 inline、新旧两套按键循环共用这一份。
//!
//! 命令打全、敲了空格之后，有参数候选的命令（目前是 `/config <分组>`）接着
//! 列参数，第一条是不带参数的命令本身，所以 `/config ` 直接回车仍是完整菜单。

use crate::cli::t;
use crate::slash_commands::{repl_command_suggestions, REPL_COMMAND_TABLE};

/// 候选里能同时看见的条数，选中项超出时窗口跟着滚。
pub(in crate::cli) const PICKER_WINDOW: usize = 4;

#[derive(Default)]
pub(in crate::cli) struct CommandPicker {
    anchor: String,
    pick: Option<usize>,
    dismissed: bool,
}

/// 一条候选：选中后放进输入框的文字、它的说明、它还收不收参数。
pub(in crate::cli) struct PickerItem {
    pub(in crate::cli) text: String,
    pub(in crate::cli) help: &'static str,
    arg_hint: &'static str,
}

/// 此刻该显示的候选与选中项。
pub(in crate::cli) struct PickerView {
    pub(in crate::cli) items: Vec<PickerItem>,
    pub(in crate::cli) selected: usize,
}

pub(in crate::cli) enum PickerKey {
    Up,
    Down,
    Tab,
    Enter,
    Esc,
}

pub(in crate::cli) enum PickerOutcome {
    /// 选中项动了，重画即可。
    Moved,
    /// 候选关了。
    Dismissed,
    /// 把这串放进输入框（光标到末尾），不提交。
    Fill(String),
    /// 把这串放进输入框并提交。
    Submit(String),
}

/// 输入对应的候选。只剩一条且已经打全了就不算（别挡着）。
pub(in crate::cli) fn picker_candidates(input: &str) -> Vec<PickerItem> {
    let typed = input.trim_start();
    let items = match typed.split_once(char::is_whitespace) {
        None => command_items(typed),
        Some((command, rest)) => argument_items(command, rest.trim_start()),
    };
    if items.len() == 1 && items[0].text == typed.trim_end() {
        return Vec::new();
    }
    items
}

fn command_items(typed: &str) -> Vec<PickerItem> {
    repl_command_suggestions(typed)
        .into_iter()
        .filter_map(|name| REPL_COMMAND_TABLE.iter().find(|spec| spec.name == name))
        .map(|spec| PickerItem {
            text: spec.name.to_string(),
            help: t(spec.help_en, spec.help_zh),
            arg_hint: spec.arg_hint,
        })
        .collect()
}

/// 命令后面的参数候选。参数里再有空白（第二个参数）就不管了。
fn argument_items(command: &str, prefix: &str) -> Vec<PickerItem> {
    if prefix.chars().any(char::is_whitespace) {
        return Vec::new();
    }
    let Some(spec) = REPL_COMMAND_TABLE.iter().find(|spec| spec.name == command) else {
        return Vec::new();
    };
    let choices = argument_choices(spec.name);
    if choices.is_empty() {
        return Vec::new();
    }
    let bare = prefix.is_empty().then(|| PickerItem {
        text: spec.name.to_string(),
        help: t(spec.help_en, spec.help_zh),
        arg_hint: "",
    });
    let prefix = prefix.to_ascii_lowercase();
    bare.into_iter()
        .chain(
            choices
                .into_iter()
                .filter(|(value, _)| value.starts_with(&prefix))
                .map(|(value, help)| PickerItem {
                    text: format!("{} {value}", spec.name),
                    help,
                    arg_hint: "",
                }),
        )
        .collect()
}

/// 有固定参数候选的命令：参数值与它的说明。
fn argument_choices(command: &str) -> Vec<(&'static str, &'static str)> {
    match command {
        "/config" => crate::config_tui::settings_group_choices(),
        _ => Vec::new(),
    }
}

impl CommandPicker {
    fn owns(&self, input: &str) -> bool {
        self.anchor == input
    }

    /// 候选开着就返回内容。默认选中和输入完全相同的那条（`/session` 同时匹配
    /// `/sessions` 时，回车仍是执行打的那条），否则第一条。
    pub(in crate::cli) fn view(&self, input: &str) -> Option<PickerView> {
        let owned = self.owns(input);
        if owned && self.dismissed {
            return None;
        }
        let items = picker_candidates(input);
        if items.is_empty() {
            return None;
        }
        let typed = input.trim();
        let default = items
            .iter()
            .position(|item| item.text == typed)
            .unwrap_or(0);
        let selected = match (owned, self.pick) {
            (true, Some(pick)) => pick.min(items.len() - 1),
            _ => default,
        };
        Some(PickerView { items, selected })
    }

    /// 候选开着时处理一个键。返回 `None` = 候选没开，按键照常走。
    pub(in crate::cli) fn handle(&mut self, input: &str, key: PickerKey) -> Option<PickerOutcome> {
        let view = self.view(input)?;
        let len = view.items.len();
        let item = &view.items[view.selected];
        let name = item.text.as_str();
        if !self.owns(input) {
            self.anchor = input.to_string();
            self.pick = None;
            self.dismissed = false;
        }
        Some(match key {
            PickerKey::Up => {
                self.pick = Some((view.selected + len - 1) % len);
                PickerOutcome::Moved
            }
            PickerKey::Down => {
                self.pick = Some((view.selected + 1) % len);
                PickerOutcome::Moved
            }
            PickerKey::Esc => {
                self.dismissed = true;
                PickerOutcome::Dismissed
            }
            PickerKey::Tab => {
                let space = if item.arg_hint.is_empty() { "" } else { " " };
                PickerOutcome::Fill(format!("{name}{space}"))
            }
            // 必填参数（`<name>`）的命令没法空着执行，填进去等你补参数。
            PickerKey::Enter if item.arg_hint.starts_with('<') => {
                PickerOutcome::Fill(format!("{name} "))
            }
            PickerKey::Enter => PickerOutcome::Submit(name.to_string()),
        })
    }

    /// Esc 专用：候选开着就关掉并返回真。
    pub(in crate::cli) fn dismiss(&mut self, input: &str) -> bool {
        matches!(
            self.handle(input, PickerKey::Esc),
            Some(PickerOutcome::Dismissed)
        )
    }
}

/// 选中项所在的可见窗口：`[start, end)`。
pub(in crate::cli) fn picker_window(len: usize, selected: usize) -> (usize, usize) {
    if len <= PICKER_WINDOW {
        return (0, len);
    }
    let start = selected
        .saturating_sub(PICKER_WINDOW - 1)
        .min(len - PICKER_WINDOW);
    (start, start + PICKER_WINDOW)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn submit_of(picker: &mut CommandPicker, input: &str) -> Option<String> {
        match picker.handle(input, PickerKey::Enter) {
            Some(PickerOutcome::Submit(text)) => Some(text),
            _ => None,
        }
    }

    #[test]
    fn arrows_move_and_wrap_then_enter_runs_the_selection() {
        let mut picker = CommandPicker::default();
        let first = picker.view("/").unwrap();
        assert_eq!(first.selected, 0);
        let len = first.items.len();
        assert!(len > 2);
        picker.handle("/", PickerKey::Up);
        assert_eq!(picker.view("/").unwrap().selected, len - 1);
        picker.handle("/", PickerKey::Down);
        picker.handle("/", PickerKey::Down);
        let view = picker.view("/").unwrap();
        assert_eq!(view.selected, 1);
        let expected = view.items[1].text.clone();
        if view.items[1].arg_hint.starts_with('<') {
            return;
        }
        assert_eq!(submit_of(&mut picker, "/"), Some(expected));
    }

    #[test]
    fn typing_resets_the_selection_and_the_dismissal() {
        let mut picker = CommandPicker::default();
        picker.handle("/", PickerKey::Down);
        assert!(picker.dismiss("/"));
        assert!(picker.view("/").is_none());
        // 又打了一个字：新的一串，候选回来、选中回到第一条。
        let view = picker.view("/c").unwrap();
        assert_eq!(view.selected, 0);
        assert!(picker.handle("/c", PickerKey::Down).is_some());
    }

    #[test]
    fn exact_match_is_selected_by_default() {
        let items = picker_candidates("/session");
        if items.len() < 2 {
            return;
        }
        let picker = CommandPicker::default();
        let view = picker.view("/session").unwrap();
        assert_eq!(view.items[view.selected].text, "/session");
    }

    #[test]
    fn closed_picker_leaves_keys_alone() {
        let mut picker = CommandPicker::default();
        assert!(picker.handle("hello", PickerKey::Up).is_none());
        assert!(picker.handle("/compact 3", PickerKey::Enter).is_none());
        assert!(!picker.dismiss(""));
    }

    #[test]
    fn tab_fills_with_a_space_only_when_the_command_takes_arguments() {
        for spec in REPL_COMMAND_TABLE {
            let mut picker = CommandPicker::default();
            // 用去掉最后一个字的前缀调出候选，把选中项挪到这条命令上。
            let prefix = &spec.name[..spec.name.len() - 1];
            let Some(view) = picker.view(prefix) else {
                continue;
            };
            let Some(index) = view.items.iter().position(|item| item.text == spec.name) else {
                continue;
            };
            for _ in 0..index {
                picker.handle(prefix, PickerKey::Down);
            }
            let Some(PickerOutcome::Fill(text)) = picker.handle(prefix, PickerKey::Tab) else {
                panic!("tab should fill {}", spec.name);
            };
            let want = if spec.arg_hint.is_empty() {
                spec.name.to_string()
            } else {
                format!("{} ", spec.name)
            };
            assert_eq!(text, want);
        }
    }

    #[test]
    fn config_lists_its_groups_after_a_space() {
        let mut picker = CommandPicker::default();
        let view = picker.view("/config ").unwrap();
        // 第一条是不带参数的 `/config`，默认选中：直接回车仍是完整菜单。
        assert_eq!(view.items[0].text, "/config");
        assert_eq!(view.selected, 0);
        assert!(view.items.iter().any(|item| item.text == "/config context"));
        assert_eq!(
            submit_of(&mut picker, "/config ").as_deref(),
            Some("/config")
        );

        let view = picker.view("/config c").unwrap();
        let texts: Vec<&str> = view.items.iter().map(|item| item.text.as_str()).collect();
        assert_eq!(texts, ["/config context", "/config cache"]);
        assert!(picker.view("/config context").is_none(), "打全了就收起");
        assert!(picker.view("/config context x").is_none());
        assert!(picker.view("/goal ").is_none(), "没有参数候选的命令不弹");
    }

    #[test]
    fn window_follows_the_selection() {
        assert_eq!(picker_window(3, 2), (0, 3));
        assert_eq!(picker_window(10, 0), (0, 4));
        assert_eq!(picker_window(10, 3), (0, 4));
        assert_eq!(picker_window(10, 4), (1, 5));
        assert_eq!(picker_window(10, 9), (6, 10));
    }
}
