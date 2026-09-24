//! 斜杠命令候选的方向键选择（`docs/plan/2026-09-23-tui-commands-settings.md` §一）。
//!
//! 规则（09-10 定）：候选开着时 ↑↓ 在候选里移动（不翻历史），Tab 把选中的
//! 命令填进输入框，Enter 直接执行选中的命令，Esc 关掉候选。
//!
//! 状态挂在「当时那一串输入」上：`anchor` 记着选择是对哪串输入做的，输入一变
//! （打字、删字、填入）选择和「已关掉」自然作废，不用在每个改输入的地方挂钩子。
//! 全屏与 inline、新旧两套按键循环共用这一份。

use crate::slash_commands::{repl_command_suggestions, REPL_COMMAND_TABLE};

/// 候选里能同时看见的条数，选中项超出时窗口跟着滚。
pub(in crate::cli) const PICKER_WINDOW: usize = 4;

#[derive(Default)]
pub(in crate::cli) struct CommandPicker {
    anchor: String,
    pick: Option<usize>,
    dismissed: bool,
}

/// 此刻该显示的候选与选中项。
pub(in crate::cli) struct PickerView {
    pub(in crate::cli) names: Vec<&'static str>,
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

/// 输入对应的候选。只剩一条且已经打全了就不算（别挡着），带了参数也不算。
pub(in crate::cli) fn picker_candidates(input: &str) -> Vec<&'static str> {
    let input = input.trim_start();
    if input.chars().any(char::is_whitespace) {
        return Vec::new();
    }
    let names = repl_command_suggestions(input);
    if names.len() == 1 && names[0] == input {
        return Vec::new();
    }
    names
}

fn arg_hint(name: &str) -> &'static str {
    REPL_COMMAND_TABLE
        .iter()
        .find(|spec| spec.name == name)
        .map(|spec| spec.arg_hint)
        .unwrap_or("")
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
        let names = picker_candidates(input);
        if names.is_empty() {
            return None;
        }
        let typed = input.trim_start();
        let default = names.iter().position(|name| *name == typed).unwrap_or(0);
        let selected = match (owned, self.pick) {
            (true, Some(pick)) => pick.min(names.len() - 1),
            _ => default,
        };
        Some(PickerView { names, selected })
    }

    /// 候选开着时处理一个键。返回 `None` = 候选没开，按键照常走。
    pub(in crate::cli) fn handle(&mut self, input: &str, key: PickerKey) -> Option<PickerOutcome> {
        let view = self.view(input)?;
        let len = view.names.len();
        let name = view.names[view.selected];
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
                let space = if arg_hint(name).is_empty() { "" } else { " " };
                PickerOutcome::Fill(format!("{name}{space}"))
            }
            // 必填参数（`<name>`）的命令没法空着执行，填进去等你补参数。
            PickerKey::Enter if arg_hint(name).starts_with('<') => {
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
        let len = first.names.len();
        assert!(len > 2);
        picker.handle("/", PickerKey::Up);
        assert_eq!(picker.view("/").unwrap().selected, len - 1);
        picker.handle("/", PickerKey::Down);
        picker.handle("/", PickerKey::Down);
        let view = picker.view("/").unwrap();
        assert_eq!(view.selected, 1);
        let expected = view.names[1];
        if arg_hint(expected).starts_with('<') {
            return;
        }
        assert_eq!(submit_of(&mut picker, "/").as_deref(), Some(expected));
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
        let names = picker_candidates("/session");
        if names.len() < 2 {
            return;
        }
        let picker = CommandPicker::default();
        let view = picker.view("/session").unwrap();
        assert_eq!(view.names[view.selected], "/session");
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
            let Some(index) = view.names.iter().position(|name| *name == spec.name) else {
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
    fn window_follows_the_selection() {
        assert_eq!(picker_window(3, 2), (0, 3));
        assert_eq!(picker_window(10, 0), (0, 4));
        assert_eq!(picker_window(10, 3), (0, 4));
        assert_eq!(picker_window(10, 4), (1, 5));
        assert_eq!(picker_window(10, 9), (6, 10));
    }
}
