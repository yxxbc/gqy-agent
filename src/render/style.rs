//! 终端配色与 ANSI 样式。
//!
//! 集中一处是为了改主题时不用全文件找。命名按用途而非颜色（`HEADER_STYLE` 而
//! 不是 `BOLD_CYAN`），换配色时才不用改调用点。
//!
//! 颜色分两类（见 [`Swatch`]）：**界面色**只发终端 16 色槽位，跟终端配色方案
//! 走；**内容色**写一次深底 RGB、一次浅底 RGB、一个 16 色兜底。进程内第一次
//! 取色时按终端底色（[`crate::terminal::tone`]）与色深（[`Depth`]）落成转义
//! 序列，之后都是同一份 `&'static str`（见 [`Ansi`]）。测试里固定深底真彩，
//! 断言不受开发机终端影响。

use std::sync::OnceLock;

use crate::terminal::palette::{to_256, Depth, Rgb};
use crate::terminal::tone::{self, Tone};

pub(crate) const RESET: &str = "\x1b[0m";

pub(crate) const STRIKE_STYLE: &str = "\x1b[9m";

pub(crate) const CODE_BLOCK_BG: &str = "";

pub(crate) const CODE_TOKEN_RESET: &str = "\x1b[0m";

/// 思考过程：弱化 + 斜体。不上色，深浅底都不抢正文。
pub(crate) const THINKING_STYLE: &str = "\x1b[2m\x1b[3m";

/// 一个颜色的落法。`ansi16` 是 SGR 参数（`"34"`），空串表示用终端默认前景。
///
/// `follow = true` 的是**界面色**（竖条、模式标签、选中标记、成败色）：不管
/// 终端多少色都只发 16 色槽位，让终端配色方案决定实际颜色——用户的 kitty
/// 16 色盘由 matugen 按壁纸生成，界面要跟着壁纸走（09-05 声波配色实录）。
/// 其余是**内容色**（代码高亮、diff、markdown 的浅灰浅蓝）：终端配色方案管
/// 不到的 256 色/真彩，按深浅底各备一套。
#[derive(Clone, Copy)]
struct Swatch {
    dark: Rgb,
    light: Rgb,
    ansi16: &'static str,
    follow: bool,
}

const fn swatch(dark: Rgb, light: Rgb, ansi16: &'static str) -> Swatch {
    Swatch {
        dark,
        light,
        ansi16,
        follow: false,
    }
}

/// 界面色：只认终端的 16 色槽位。
const fn slot(ansi16: &'static str) -> Swatch {
    Swatch {
        dark: (0, 0, 0),
        light: (0, 0, 0),
        ansi16,
        follow: true,
    }
}

/// 色值表。只在这里写 RGB。
mod swatches {
    use super::{slot, swatch, Swatch};

    // ── 界面色（跟终端配色方案走） ──
    pub(super) const ACCENT: Swatch = slot("34");
    pub(super) const ACCENT_DEV: Swatch = slot("35");
    pub(super) const SUCCESS: Swatch = slot("32");
    pub(super) const WARNING: Swatch = slot("33");
    pub(super) const DANGER: Swatch = slot("31");
    pub(super) const INFO: Swatch = slot("36");
    /// 正文里的次级强调（列表符号、标题、占位符）。
    pub(super) const TERTIARY: Swatch = slot("35");
    /// 淡灰（diff 行号与上下文行、装饰竖条）。
    pub(super) const FAINT: Swatch = slot("90");

    // ── 内容色（深浅两套） ──
    /// 比正文淡一档的文字（路径、斜体）。
    pub(super) const SOFT: Swatch = swatch((0xbc, 0xbc, 0xbc), (0x4a, 0x4a, 0x4a), "");

    // ── markdown ──
    pub(super) const PRIMARY: Swatch = swatch((0xd7, 0xd7, 0xff), (0x3a, 0x3a, 0x6a), "");
    pub(super) const LINK_LABEL: Swatch = swatch((0x87, 0xd7, 0xff), (0x1f, 0x5f, 0xb0), "34");
    pub(super) const URL: Swatch = swatch((0x5f, 0xaf, 0xff), (0x2a, 0x6a, 0xc0), "34");
    pub(super) const IMAGE: Swatch = swatch((0xd7, 0xaf, 0xff), (0x80, 0x4a, 0xb0), "35");

    // ── 代码高亮 ──
    pub(super) const CODE_KEYWORD: Swatch = swatch((196, 167, 231), (0x7a, 0x3e, 0xb0), "35");
    pub(super) const CODE_FUNCTION: Swatch = swatch((156, 207, 216), (0x1f, 0x6f, 0x8b), "36");
    pub(super) const CODE_STRING: Swatch = swatch((166, 214, 160), (0x3a, 0x7d, 0x2f), "32");
    pub(super) const CODE_NUMBER: Swatch = swatch((246, 193, 119), (0x9a, 0x5b, 0x00), "33");
    pub(super) const CODE_COMMENT: Swatch = slot("32");

    // ── diff：前景 + 底色成对 ──
    pub(super) const DELETE_FG: Swatch = swatch((0xff, 0x87, 0x87), (0xa3, 0x1d, 0x2b), "31");
    pub(super) const DELETE_BG: Swatch = swatch((60, 41, 53), (0xfb, 0xe4, 0xe6), "");
    pub(super) const INSERT_FG: Swatch = swatch((0xaf, 0xff, 0xaf), (0x1d, 0x6b, 0x2f), "32");
    pub(super) const INSERT_BG: Swatch = swatch((32, 52, 67), (0xe2, 0xf2, 0xe5), "");

    /// 全屏 TUI 展开区的底色。比终端背景深（浅底下是略暗的米白）一档，看得出
    /// 层次，又不至于像另开了一个控件。深底值即原来的 256 色 236。
    pub(super) const EXPANSION_BG: Swatch = swatch((0x30, 0x30, 0x30), (0xec, 0xec, 0xe8), "");
}

#[derive(Clone, Copy)]
struct Target {
    tone: Tone,
    depth: Depth,
}

impl Target {
    fn detect() -> Self {
        if cfg!(test) {
            return Self {
                tone: Tone::Dark,
                depth: Depth::True,
            };
        }
        Self {
            tone: tone::current(),
            depth: Depth::detect(),
        }
    }

    fn rgb(self, swatch: Swatch) -> Rgb {
        match self.tone {
            Tone::Dark => swatch.dark,
            Tone::Light => swatch.light,
        }
    }

    /// 前景色转义。16 色档用兜底色号，不上色档返回空串。
    fn fg(self, swatch: Swatch) -> String {
        if swatch.follow && self.depth != Depth::Mono {
            return format!("\x1b[{}m", swatch.ansi16);
        }
        let (r, g, b) = self.rgb(swatch);
        match self.depth {
            Depth::True => format!("\x1b[38;2;{r};{g};{b}m"),
            Depth::X256 => format!("\x1b[38;5;{}m", to_256((r, g, b))),
            Depth::Ansi16 if !swatch.ansi16.is_empty() => format!("\x1b[{}m", swatch.ansi16),
            Depth::Ansi16 | Depth::Mono => String::new(),
        }
    }

    /// 底色转义。16 色以下没有像样的浅/深底，干脆不铺。
    fn bg(self, swatch: Swatch) -> String {
        let (r, g, b) = self.rgb(swatch);
        match self.depth {
            Depth::True => format!("\x1b[48;2;{r};{g};{b}m"),
            Depth::X256 => format!("\x1b[48;5;{}m", to_256((r, g, b))),
            Depth::Ansi16 | Depth::Mono => String::new(),
        }
    }

    fn ratatui_bg(self, swatch: Swatch) -> ratatui::style::Color {
        use ratatui::style::Color;
        let (r, g, b) = self.rgb(swatch);
        match self.depth {
            Depth::True => Color::Rgb(r, g, b),
            Depth::X256 => Color::Indexed(to_256((r, g, b))),
            Depth::Ansi16 | Depth::Mono => Color::Reset,
        }
    }
}

pub(crate) struct Palette {
    accent: String,
    accent_dev: String,
    muted: String,
    success: String,
    warning: String,
    danger: String,
    danger_dim: String,
    info: String,
    tertiary: String,
    soft: String,
    faint: String,
    primary: String,
    header: String,
    bold: String,
    italic: String,
    link_label: String,
    url: String,
    image: String,
    code_keyword: String,
    code_function: String,
    code_string: String,
    code_number: String,
    code_comment: String,
    patch_delete: String,
    patch_insert: String,
    expansion_bg: ratatui::style::Color,
}

impl Palette {
    fn build(target: Target) -> Self {
        use swatches as sw;
        let fg = |swatch| target.fg(swatch);
        Self {
            accent: fg(sw::ACCENT),
            accent_dev: fg(sw::ACCENT_DEV),
            muted: "\x1b[2m".to_string(),
            success: fg(sw::SUCCESS),
            warning: fg(sw::WARNING),
            danger: fg(sw::DANGER),
            danger_dim: format!("\x1b[2m{}", fg(sw::DANGER)),
            info: fg(sw::INFO),
            tertiary: fg(sw::TERTIARY),
            soft: fg(sw::SOFT),
            faint: fg(sw::FAINT),
            primary: fg(sw::PRIMARY),
            header: format!("\x1b[1m{}", fg(sw::TERTIARY)),
            bold: format!("\x1b[1m{}", fg(sw::ACCENT)),
            italic: format!("\x1b[3m{}", fg(sw::SOFT)),
            link_label: fg(sw::LINK_LABEL),
            url: format!("\x1b[2m{}", fg(sw::URL)),
            image: fg(sw::IMAGE),
            code_keyword: fg(sw::CODE_KEYWORD),
            code_function: fg(sw::CODE_FUNCTION),
            code_string: fg(sw::CODE_STRING),
            code_number: fg(sw::CODE_NUMBER),
            code_comment: fg(sw::CODE_COMMENT),
            patch_delete: format!("{}{}", target.bg(sw::DELETE_BG), fg(sw::DELETE_FG)),
            patch_insert: format!("{}{}", target.bg(sw::INSERT_BG), fg(sw::INSERT_FG)),
            expansion_bg: target.ratatui_bg(sw::EXPANSION_BG),
        }
    }
}

fn palette() -> &'static Palette {
    static PALETTE: OnceLock<Palette> = OnceLock::new();
    PALETTE.get_or_init(|| Palette::build(Target::detect()))
}

/// 一个具名样式：第一次用到时才按终端落成转义序列。
///
/// 能直接进 `format!("{URL_STYLE}{url}{RESET}")`，也能当 `&str` 用
/// （`push_str(&PRIMARY_STYLE)`）——调用点和以前的字符串常量写法一样。
#[derive(Clone, Copy)]
pub(crate) struct Ansi(fn(&'static Palette) -> &'static str);

impl Ansi {
    pub(crate) fn as_str(self) -> &'static str {
        (self.0)(palette())
    }
}

impl std::ops::Deref for Ansi {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for Ansi {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

macro_rules! styles {
    ($($(#[$meta:meta])* $name:ident => $field:ident),* $(,)?) => {
        $(
            $(#[$meta])*
            pub(crate) const $name: Ansi = Ansi(|palette| &palette.$field);
        )*
    };
}

styles!(
    /// 普通模式主色：输入框竖条、模式标签、选中标记、spinner。
    ACCENT => accent,
    /// 开发模式主色。
    ACCENT_DEV => accent_dev,
    /// 弱化的说明文字、分隔符：SGR 2，不上色。
    MUTED => muted,
    SUCCESS => success,
    WARNING => warning,
    DANGER => danger,
    /// stderr、失败命令的弱化红。
    DANGER_DIM => danger_dim,
    INFO => info,
    /// 比正文淡一档（路径）。
    SOFT => soft,
    /// 再淡一档（diff 行号、上下文行、装饰竖条）。
    FAINT => faint,
    PRIMARY_STYLE => primary,
    TERTIARY_STYLE => tertiary,
    HEADER_STYLE => header,
    /// 行内代码、代码块边框与标签。
    INLINE_CODE_STYLE => info,
    CODE_BLOCK_FRAME_STYLE => info,
    BOLD_STYLE => bold,
    ITALIC_STYLE => italic,
    LINK_LABEL_STYLE => link_label,
    URL_STYLE => url,
    IMAGE_STYLE => image,
    CODE_KEYWORD_STYLE => code_keyword,
    CODE_FUNCTION_STYLE => code_function,
    CODE_STRING_STYLE => code_string,
    CODE_NUMBER_STYLE => code_number,
    CODE_COMMENT_STYLE => code_comment,
    PATCH_DELETE_STYLE => patch_delete,
    PATCH_INSERT_STYLE => patch_insert,
);

/// 全屏 TUI 展开区（工具输出、思考正文）的底色。
pub(crate) fn expansion_bg() -> ratatui::style::Color {
    palette().expansion_bg
}

/// 按模式取主色。
pub(crate) fn mode_accent(dev: bool) -> Ansi {
    if dev {
        ACCENT_DEV
    } else {
        ACCENT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(tone: Tone, depth: Depth) -> Palette {
        Palette::build(Target { tone, depth })
    }

    #[test]
    fn tone_switches_diff_background() {
        let dark = target(Tone::Dark, Depth::True);
        let light = target(Tone::Light, Depth::True);
        assert!(dark.patch_insert.starts_with("\x1b[48;2;32;52;67m"));
        assert_ne!(dark.patch_insert, light.patch_insert);
        assert_ne!(dark.code_keyword, light.code_keyword);
    }

    #[test]
    fn low_depth_falls_back_to_ansi16_then_nothing() {
        let sixteen = target(Tone::Dark, Depth::Ansi16);
        assert_eq!(sixteen.accent, "\x1b[34m");
        assert_eq!(sixteen.danger, "\x1b[31m");
        assert_eq!(sixteen.patch_delete, "\x1b[31m");
        assert_eq!(sixteen.primary, "");
        let mono = target(Tone::Light, Depth::Mono);
        assert_eq!(mono.accent, "");
        assert_eq!(mono.muted, "\x1b[2m");
        assert_eq!(mono.danger, "");
        assert_eq!(mono.patch_insert, "");
        let x256 = target(Tone::Dark, Depth::X256);
        assert!(x256.code_keyword.starts_with("\x1b[38;5;"));
    }

    #[test]
    fn test_builds_use_dark_truecolor() {
        // 界面色即便在真彩终端也只发 16 色槽位。
        assert_eq!(&*ACCENT, "\x1b[34m");
        assert_eq!(&*HEADER_STYLE, "\x1b[1m\x1b[35m");
        assert_eq!(&*mode_accent(true), &*ACCENT_DEV);
        assert_eq!(format!("{DANGER}x"), format!("{}x", DANGER.as_str()));
    }
}
