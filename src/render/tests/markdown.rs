//! Markdown 行内与块级渲染。

use crate::render::*;

#[test]
fn streams_only_complete_lines() {
    let mut renderer = MarkdownStreamRenderer::new();
    assert_eq!(renderer.push("**bo"), "");
    assert_eq!(
        renderer.push("ld**\n"),
        format!("{BOLD_STYLE}bold{RESET}\n")
    );
}

#[test]
fn flushes_partial_final_line() {
    let mut renderer = MarkdownStreamRenderer::new();
    assert_eq!(renderer.push("# Title"), "");
    assert_eq!(renderer.flush(), format!("{HEADER_STYLE}# Title{RESET}\n"));
}

#[test]
fn headings_use_one_color_and_distinct_prefix_lengths() {
    assert_eq!(
        render_markdown_line("# One"),
        format!("{HEADER_STYLE}# One{RESET}")
    );
    assert_eq!(
        render_markdown_line("## Two"),
        format!("{HEADER_STYLE}## Two{RESET}")
    );
    assert_eq!(
        render_markdown_line("### Three"),
        format!("{HEADER_STYLE}### Three{RESET}")
    );
    assert_eq!(
        render_markdown_line("###### Six"),
        format!("{HEADER_STYLE}###### Six{RESET}")
    );
}

#[test]
fn list_markers_use_tertiary_color() {
    assert!(render_markdown_line("- item").contains(&format!("{TERTIARY_STYLE}-{RESET}")));
    assert!(render_markdown_line("1. item").contains(&format!("{TERTIARY_STYLE}1.{RESET}")));
}

#[test]
fn blockquote_is_visually_distinct() {
    let mut renderer = MarkdownStreamRenderer::new();
    let output = renderer.push(">> quoted\n");
    assert!(output.contains("\x1b[32m| \x1b[0m\x1b[32m| \x1b[0m"));
    assert!(output.contains("\x1b[32mquoted\x1b[0m"));
    assert!(!output.contains("48;5;236"));
}

#[test]
fn code_block_has_label_and_readable_content() {
    let mut renderer = MarkdownStreamRenderer::new();
    let output = renderer.push("```rust\nfn main() {}\n```\n");
    assert!(output.contains("╭─ code rust"));
    assert!(!output.contains(",-- code rust"));
    assert!(!output.contains("\x1b[2m|\x1b[0m"));
    assert!(output.contains(&format!(
        "{CODE_BLOCK_BG}{CODE_KEYWORD_STYLE}fn{CODE_TOKEN_RESET}"
    )));
    assert!(output.contains(&format!("{CODE_FUNCTION_STYLE}main{CODE_TOKEN_RESET}")));
    assert!(output.contains(&format!("{CODE_BLOCK_FRAME_STYLE}╭─ code rust ─")));
    assert!(output.contains(&format!(
        "{CODE_BLOCK_FRAME_STYLE}{}{RESET}",
        "─".repeat(24)
    )));
    assert!(!output.contains("`--"));
}

#[test]
fn code_block_content_has_default_color() {
    let mut renderer = MarkdownStreamRenderer::new();
    let output = renderer.push("```\nXMODIFIERS \"@im=fcitx\"\n```\n");
    assert!(output.contains(&format!(
        "{CODE_BLOCK_BG}XMODIFIERS \"@im=fcitx\"{}{RESET}",
        " ".repeat(2)
    )));
    assert!(!output.contains("\x1b[33mXMODIFIERS"));
}

#[test]
fn code_block_variables_use_primary_color() {
    let mut renderer = MarkdownStreamRenderer::new();
    let output = renderer.push("```rust\nlet msg = String::from(\"hi\");\n```\n");
    assert!(output.contains(&format!("{PRIMARY_STYLE}msg{CODE_TOKEN_RESET}")));
}

#[test]
fn code_block_background_uses_longest_line_width() {
    let mut renderer = MarkdownStreamRenderer::new();
    let output = renderer.push("```\nshort\nlonger line\n```\n");
    assert!(output.contains(&format!("{CODE_BLOCK_BG}short{}{RESET}", " ".repeat(19))));
    assert!(output.contains(&format!(
        "{CODE_BLOCK_BG}longer line{}{RESET}",
        " ".repeat(13)
    )));
    assert!(output.contains(&format!(
        "{CODE_BLOCK_FRAME_STYLE}{}{RESET}",
        "─".repeat(24)
    )));
    assert!(!output.contains("48;5;236"));
}

#[test]
fn renders_more_inline_markdown() {
    let output = render_inline(
        "*i* ~~gone~~ [site](https://example.com) <https://example.org> ![pic](https://img)",
    );
    assert!(output.contains(&format!("{ITALIC_STYLE}i{RESET}")));
    assert!(output.contains(&format!("{STRIKE_STYLE}gone{RESET}")));
    assert!(output.contains(&format!("<{URL_STYLE}https://example.com{RESET}>")));
    assert!(output.contains(&format!(
        "\x1b[4m<{URL_STYLE}https://example.org{RESET}>{RESET}"
    )));
    assert!(output.contains(&format!(
        "{IMAGE_STYLE}[image: pic]{RESET}({URL_STYLE}https://img{RESET})"
    )));
    assert!(!output.contains("\x1b[35mimage\x1b[0m"));
}

#[test]
fn renders_inline_code_at_start_of_bullet() {
    let output = render_markdown_line("- `read_file` — 读文件内容");
    assert!(output.contains(&format!("{INLINE_CODE_STYLE}read_file\x1b[0m")));
    assert!(output.contains("— 读文件内容"));
}

#[test]
fn renders_multiple_inline_code_spans_in_bullet_with_chinese_text() {
    let output = render_markdown_line(
        "- `~/.config/Thunar/` - 里面有 `accels.scm`（快捷键绑定）和 `uca.xml`（自定义右键菜单）",
    );
    assert!(output.contains(&format!("{INLINE_CODE_STYLE}~/.config/Thunar/\x1b[0m")));
    assert!(output.contains(&format!("{INLINE_CODE_STYLE}accels.scm\x1b[0m")));
    assert!(output.contains(&format!("{INLINE_CODE_STYLE}uca.xml\x1b[0m")));
    assert!(!output.contains('`'));
}

#[test]
fn renders_inline_code_when_stream_chunks_split_backticks() {
    let mut renderer = MarkdownStreamRenderer::new();
    assert_eq!(renderer.push("- `~/.config/Thu"), "");
    let output = renderer.push("nar/` - 里面有 `accels.scm`\n");
    assert!(output.contains(&format!("{INLINE_CODE_STYLE}~/.config/Thunar/\x1b[0m")));
    assert!(output.contains(&format!("{INLINE_CODE_STYLE}accels.scm\x1b[0m")));
    assert!(!output.contains('`'));
}

#[test]
fn renders_double_backtick_span_whose_content_contains_a_backtick() {
    // 模型写 ``a`b`` 时闭合必须认**等长**的反引号串：拿下一个单反引号当闭合的话，
    // 只会得到一个空的着色段，剩下的反引号原样漏在屏幕上。
    let output = render_inline("记号 ``a`b`` 要整段上色");
    assert_eq!(
        output,
        format!("记号 {INLINE_CODE_STYLE}a`b{RESET} 要整段上色"),
        "{output}"
    );
    // 单反引号的旧行为不能动：整行不该留下定界符。
    assert_eq!(render_inline("`x` 与 `y`"), format!("{INLINE_CODE_STYLE}x{RESET} 与 {INLINE_CODE_STYLE}y{RESET}"));
}

#[test]
fn keeps_unclosed_backtick_run_literal_without_eating_the_line() {
    // 这一行找不到等长的闭合串：那串反引号原样吐出来，后面真的 `x` 照样上色。
    let output = render_inline("``` 开头没闭合,后面 `x` 还在");
    assert!(output.starts_with("``` "), "{output}");
    assert!(output.contains(&format!("{INLINE_CODE_STYLE}x{RESET}")), "{output}");
}

#[test]
fn styles_inline_code_inside_link_label() {
    let output = render_inline("[读 `x.y.z`](https://example.com/a)");
    assert!(
        output.contains(&format!("{LINK_LABEL_STYLE}读 {INLINE_CODE_STYLE}x.y.z")),
        "{output}"
    );
    assert!(!output.contains('`'), "标签里的反引号要被吃掉: {output}");
    assert!(
        output.contains(&format!("{URL_STYLE}https://example.com/a{RESET}")),
        "{output}"
    );
}

#[test]
fn fenced_code_blocks_keep_backticks_literal() {
    // 围栏里的反引号轮不到行内解析：这条守住那条边界。
    let mut renderer = MarkdownStreamRenderer::new();
    let output = renderer.push("```\nlet x = `date`;\n```\n");
    let plain = strip_ansi_text(&output);
    assert!(plain.contains("let x = `date`;"), "{plain}");
}

#[test]
fn keeps_identifier_underscores_literal() {
    let output = render_inline("GTK_IM_MODULE and _italic_");
    assert!(output.contains("GTK_IM_MODULE"));
    assert!(output.contains(&format!("{ITALIC_STYLE}italic{RESET}")));
    assert!(!output.contains("GTK\x1b[3mIM\x1b[0mMODULE"));
    assert_eq!(render_inline("abc_def_ghi"), "abc_def_ghi");
}

#[test]
fn renders_math_formulas_visibly() {
    let output = render_inline("inline $E=mc^2$ and display $$a^2+b^2=c^2$$");
    assert!(output.contains("E=mc²"), "{output}");
    assert!(output.contains("a²+b²=c²"), "{output}");
    assert!(!output.contains("$E"), "raw tex must be replaced: {output}");
}

#[test]
fn renders_multiline_math_blocks_visibly() {
    let mut renderer = MarkdownStreamRenderer::new();
    let output = renderer.push("$$\na^2 + b^2 = c^2\n$$\n");
    assert!(
        output.contains('▀') || output.contains('▄'),
        "block math should render halfblocks: {output}"
    );
    assert!(!output.contains("a^2"), "{output}");
}

#[test]
fn renders_selected_inline_html_tags() {
    let output = render_inline("<u>under</u> H<sub>2</sub> x<sup>2</sup><br>next");
    assert!(output.contains("\x1b[4munder\x1b[0m"));
    assert!(output.contains("H\x1b[2m2\x1b[0m"));
    assert!(output.contains("x\x1b[1m2\x1b[0m"));
    assert!(output.contains("\nnext"));
}

#[test]
fn horizontal_rule_uses_terminal_width_fallback() {
    let output = render_markdown_line("---");
    assert!(output.starts_with("\x1b[2m"));
    assert!(output.ends_with("\x1b[0m"));
    assert!(visible_width(&output) >= 16);
}

// ── 链接 ──────────────────────────────────────────────────────────
// 09-09：模型给参考资料写的是纯文本 `标题 (地址)`，正文里的地址也是裸写的。
// 修之前这两样一点颜色都没有：只有 `[x](y)` 和 `<url>` 被认出来。

#[test]
fn colors_bare_urls_in_prose() {
    let output = render_inline("见 https://example.com/a 那篇");
    assert!(
        output.contains(&format!("{URL_STYLE}https://example.com/a{RESET}")),
        "{output}"
    );
}

#[test]
fn bare_url_stops_before_sentence_punctuation() {
    let output = render_inline("见 https://example.com。");
    assert!(
        output.contains(&format!("{URL_STYLE}https://example.com{RESET}。")),
        "{output}"
    );
}

#[test]
fn bare_url_stops_before_cjk_punctuation_inside() {
    // 顿号后面还跟着字母，只在末尾修剪碰不到它。
    let output = render_inline("archlinux.org 见 https://a.org、AUR");
    assert!(
        output.contains(&format!("{URL_STYLE}https://a.org{RESET}、AUR")),
        "{output}"
    );
}

#[test]
fn bare_url_keeps_balanced_parens() {
    let output = render_inline("https://en.wikipedia.org/wiki/Foo_(bar)");
    assert!(
        output.contains(&format!(
            "{URL_STYLE}https://en.wikipedia.org/wiki/Foo_(bar){RESET}"
        )),
        "{output}"
    );
}

#[test]
fn does_not_link_glued_scheme() {
    let output = render_inline("xhttps://example.com");
    assert!(!output.contains(&*URL_STYLE), "{output}");
}

#[test]
fn title_url_line_colors_the_title_too() {
    let output = render_markdown_line("Efficient LLM Collaboration (https://arxiv.org/abs/2506)");
    assert!(
        output.contains(&format!(
            "{LINK_LABEL_STYLE}Efficient LLM Collaboration{RESET}"
        )),
        "{output}"
    );
    assert!(
        output.contains(&format!("{URL_STYLE}https://arxiv.org/abs/2506{RESET}")),
        "{output}"
    );
}

#[test]
fn title_url_line_works_inside_list_items() {
    let output = render_markdown_line("- 计划式协作 (https://arxiv.org/abs/2506)");
    assert!(
        output.contains(&format!("{LINK_LABEL_STYLE}计划式协作{RESET}")),
        "{output}"
    );
}

#[test]
fn title_url_line_ignores_prose_parentheses() {
    let output = render_markdown_line("这句话 (只是个注解)");
    assert!(!output.contains(&*LINK_LABEL_STYLE), "{output}");
}

#[test]
fn file_scheme_links_are_recognized() {
    let output = render_inline("[bilibili-summary](file:///home/u/.gqy/mcp-servers/bili)");
    assert!(
        output.contains(&format!("{LINK_LABEL_STYLE}bilibili-summary{RESET}")),
        "{output}"
    );
    assert!(
        output.contains("file:///home/u/.gqy/mcp-servers/bili"),
        "{output}"
    );
    assert!(!output.contains("]("), "markdown 原文不该漏出来: {output}");
}

#[test]
fn osc8_wraps_body_in_hyperlink_escape() {
    assert_eq!(
        osc8("https://a.com", "文本"),
        "\x1b]8;;https://a.com\x1b\\文本\x1b]8;;\x1b\\"
    );
}

#[test]
fn title_url_line_leaves_markdown_links_alone() {
    // `[label](url)` 独占一行时,结尾也是 `)`——按「标题 (地址)」处理的话整条
    // Markdown 会被当成标题原样漏出来(09-09 走查抓到)。
    let output = render_markdown_line("[GitHub 上的 顾清影](https://github.com/x/y)");
    assert!(!output.contains("]("), "{output}");
    assert!(
        output.contains(&format!("{LINK_LABEL_STYLE}GitHub 上的 顾清影{RESET}")),
        "{output}"
    );
}

#[test]
fn bare_url_stops_before_cjk_full_stop_followed_by_text() {
    // 09-10 截图原句:句号后面还有整段中文,地址只到斜杠为止。
    let output = render_inline("干净地址 http://127.0.0.1:3080/。昨天那次认证签的 cookie token。");
    assert!(
        output.contains(&format!("{URL_STYLE}http://127.0.0.1:3080/{RESET}。昨天")),
        "{output}"
    );
}
