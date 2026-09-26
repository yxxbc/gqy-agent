//! `web/provider-icons.js`(供应商品牌图标表)的自检。
//!
//! WebUI 没有 JS 测试框架,这里只读字面量:图标表(每行一个单行字符串)、
//! id/协议规则表、域名规则表。断言:每个图标是完整的 `<svg …>…</svg>`、svg
//! 内部 `url(#id)` 都有同名定义、`id` 带 lobe-icons 前缀(不污染页面)、规则表
//! 指向的键都存在、来源与许可头注释在。CI 没有 node,测试也不执行 JS。

/// build.rs 会把 `web/` 下所有文件编进二进制;这里直接读源文件。
const SOURCE: &str = include_str!("../../../web/provider-icons.js");

/// 提取 `const ICONS = { … };` 里的 `键: '<svg …>',` 行。
fn icon_entries() -> Vec<(String, String)> {
    let mut entries = Vec::new();
    let mut inside = false;
    for line in SOURCE.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("const ICONS = {") {
            inside = true;
            continue;
        }
        if inside && trimmed == "};" {
            break;
        }
        if !inside || !trimmed.ends_with("',") {
            continue;
        }
        let Some((key, rest)) = trimmed.split_once(": '") else {
            panic!("icon line is not `key: '<svg…>',`: {trimmed}");
        };
        let svg = rest
            .strip_suffix("',")
            .unwrap_or_else(|| panic!("icon line does not end in `',`: {trimmed}"));
        entries.push((key.to_string(), svg.to_string()));
    }
    entries
}

/// 提取 `const <name> = { … };` 或 `const <name> = [ … ];` 里的字符串对。
fn pair_entries(header: &str, close: &str) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    let mut inside = false;
    for line in SOURCE.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(header) {
            inside = true;
            continue;
        }
        if inside && trimmed == close {
            break;
        }
        if !inside || trimmed.is_empty() || trimmed.starts_with("/*") || trimmed.starts_with('*') {
            continue;
        }
        let quoted: Vec<&str> = trimmed.split('"').skip(1).step_by(2).collect();
        assert_eq!(
            quoted.len(),
            2,
            "rule line must carry exactly two strings: {trimmed}"
        );
        entries.push((quoted[0].to_string(), quoted[1].to_string()));
    }
    entries
}

#[test]
fn every_icon_is_a_complete_svg() {
    let icons = icon_entries();
    assert!(icons.len() >= 20, "icon table looks truncated");
    for (key, svg) in &icons {
        assert!(svg.starts_with("<svg"), "{key}: not an <svg>");
        assert!(svg.ends_with("</svg>"), "{key}: unterminated <svg>");
        assert!(svg.contains("viewBox="), "{key}: no viewBox");
        // svg 内引用到的遮罩/渐变必须在同一个 svg 里定义,否则图标会瞎掉。
        for (index, _) in svg.match_indices("url(#") {
            let rest = &svg[index + 5..];
            let id = rest.split(')').next().unwrap_or_default();
            assert!(
                svg.contains(&format!("id=\"{id}\"")),
                "{key}: url(#{id}) has no matching id"
            );
        }
        // 内联进设置页的元素 id 必须带前缀,免得和页面其它节点撞车。
        for (index, _) in svg.match_indices("id=\"") {
            let rest = &svg[index + 4..];
            let id = rest.split('"').next().unwrap_or_default();
            assert!(
                id.starts_with("lobe-icons-"),
                "{key}: element id {id} is not namespaced"
            );
        }
    }
}

#[test]
fn rules_only_point_at_icons_that_exist() {
    let icons: Vec<String> = icon_entries().into_iter().map(|(key, _)| key).collect();
    let has = |key: &str| icons.iter().any(|known| known == key);

    let ids = pair_entries("const idIcons = {", "};");
    assert!(!ids.is_empty());
    for (id, icon) in &ids {
        assert!(has(icon), "id rule {id} points at missing icon {icon}");
    }
    let hosts = pair_entries("const hostRules = [", "];");
    assert!(!hosts.is_empty());
    for (host, icon) in &hosts {
        assert!(has(icon), "host rule {host} points at missing icon {icon}");
    }

    // 内置 CLI 中转线与文档点名的几家的直配钉住(改名/漏配在这里报红)。
    for (id, icon) in [
        ("claude-code", "claude"),
        ("claude", "claude"),
        ("cline", "cline"),
        ("codex", "codex"),
        ("antigravity", "antigravity"),
        ("openai", "openai"),
        ("xiaomi", "xiaomimimo"),
        ("opencodezen", "opencode"),
        ("moonshot", "kimi"),
        ("grok", "xai"),
    ] {
        assert!(
            ids.iter().any(|(key, value)| key == id && value == icon),
            "missing id rule {id} -> {icon}"
        );
    }
    for (host, icon) in [
        ("deepseek.com", "deepseek"),
        ("volces.com", "doubao"),
        ("bigmodel.cn", "zhipu"),
        ("dashscope.aliyuncs.com", "qwen"),
        ("localhost:11434", "ollama"),
        ("localhost:1234", "lmstudio"),
    ] {
        assert!(
            hosts
                .iter()
                .any(|(key, value)| key == host && value == icon),
            "missing host rule {host} -> {icon}"
        );
    }
}

#[test]
fn the_source_and_licence_are_pinned_in_the_header() {
    assert!(SOURCE.contains("@lobehub/icons-static-svg 1.95.1"));
    assert!(SOURCE.contains("MIT"));
    assert!(SOURCE.contains("window.GqyProviderIcons = { providerIcon, providerMark }"));
}
