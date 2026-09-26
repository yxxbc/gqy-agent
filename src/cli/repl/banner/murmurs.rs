//! 欢迎框下面那行碎碎念：人格随口说一句，每次开屏随机挑一句。
//!
//! 文案跟人格走：人格目录下放 `murmurs.txt`（一行一句，`#` 开头是注释）就用自己的；
//! 没有这个文件时用内置的顾清影那几句。换人格换文案，不用改代码。

use crate::config::{persona_scope_name, AppConfig};
use crate::i18n::text as t;
use crate::paths::GqyPaths;

/// 人格目录下这个文件的名字。
pub(in crate::cli) const MURMURS_FILE: &str = "murmurs.txt";

/// 内置文案（顾清影）。中文是她自己的话，英文是给英文界面用的同一份意思。
const BUILTIN: &[(&str, &str)] = &[
    (
        "I'm here — start whenever you're ready.",
        "我在，你想开始就开始。",
    ),
    (
        "I'd like to hear what your day has been like.",
        "今天过得怎么样，说给我听听。",
    ),
    (
        "Say the thing that's stuck — it halves on the way out.",
        "卡住的事说出来，就小了一半。",
    ),
    ("Change your tea once it goes cold.", "茶凉了记得换一杯。"),
    ("Did you sleep enough last night?", "昨晚睡够了吗？"),
    (
        "Don't stay up late. I've seen how that ends.",
        "别熬夜，熬夜的后果我见过。",
    ),
    (
        "The light by the window is nice — take a look?",
        "窗边的光很好，你要不要也看一眼。",
    ),
    (
        "A pointless first sentence is fine too.",
        "先说一句废话也行。",
    ),
    (
        "I kept turning last conversation over in my head.",
        "上次聊的东西我又想了想。",
    ),
    (
        "No rush. We move when you're ready.",
        "不急，你想清楚了我们再走。",
    ),
];

/// 这一次开屏说哪句。挑完就存进欢迎框，整个会话期间不再变（随机只在装载时一次）。
pub(in crate::cli) fn pick(config: &AppConfig, paths: &GqyPaths) -> Option<String> {
    let lines = lines(config, paths);
    if lines.is_empty() {
        return None;
    }
    let index = usize::try_from(rand::random::<u32>() % lines.len() as u32).unwrap_or(0);
    Some(lines[index].clone())
}

/// 这个人格能说出口的所有句子：人格目录里的文件优先，读不到就用内置的。
fn lines(config: &AppConfig, paths: &GqyPaths) -> Vec<String> {
    if let Some(personal) = from_persona_dir(config, paths) {
        if !personal.is_empty() {
            return personal;
        }
    }
    BUILTIN
        .iter()
        .map(|(en, zh)| t(en, zh).to_string())
        .collect()
}

fn from_persona_dir(config: &AppConfig, paths: &GqyPaths) -> Option<Vec<String>> {
    let persona = config.prompt.active_persona.trim();
    if persona.is_empty() {
        return None;
    }
    let file = paths
        .personas_dir()
        .join(persona_scope_name(persona))
        .join(MURMURS_FILE);
    parse_murmurs(&std::fs::read_to_string(file).ok()?)
}

/// 一行一句，`#` 开头是注释。供人格目录文件用（写坏了不报错，只是没内容）。
fn parse_murmurs(text: &str) -> Option<Vec<String>> {
    let lines: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(ToString::to_string)
        .collect();
    (!lines.is_empty()).then_some(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_lines_are_said_in_the_active_language() {
        assert!(BUILTIN.len() >= 6);
        assert!(BUILTIN
            .iter()
            .all(|(en, zh)| !en.trim().is_empty() && !zh.trim().is_empty()));
        let sample = t(BUILTIN[0].0, BUILTIN[0].1);
        assert!(!sample.trim().is_empty());
    }

    #[test]
    fn persona_file_skips_blanks_and_comments() {
        let parsed = parse_murmurs("# 顾清影的碎碎念\n\n  第一句 \n# 注释\n第二句\n").unwrap();
        assert_eq!(parsed, ["第一句", "第二句"]);
        assert!(parse_murmurs("# 只有注释\n\n").is_none());
    }
}
