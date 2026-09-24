//! 前端资源引用的完整性：页面和模块里写到的同源路径，必须真的在嵌入表里。
//!
//! 资源表由 build.rs 扫描生成，漏登记不会再发生；会发生的是反过来——文件改名
//! 或搬家后引用没跟上。那种错编译照过，只在浏览器里 404，所以在这里拦。

use std::collections::BTreeSet;
use std::path::Path;

use crate::web::{embedded_paths, versioned_index_for_test};

/// 不在嵌入表、但有专门 handler 的同源路径。
const SPECIAL_ROUTES: &[&str] = &["/theme.css", "/fence-frame.html"];

fn known_paths() -> BTreeSet<&'static str> {
    embedded_paths()
        .chain(SPECIAL_ROUTES.iter().copied())
        .collect()
}

/// 抽出 `attr="/..."` 形式的同源引用，去掉查询串。`//host` 是跨域，不算。
fn same_origin_refs(html: &str) -> Vec<String> {
    let mut refs = Vec::new();
    for attribute in ["src=\"", "href=\""] {
        let mut rest = html;
        while let Some(start) = rest.find(attribute) {
            rest = &rest[start + attribute.len()..];
            let end = rest.find('"').unwrap_or(rest.len());
            let value = &rest[..end];
            if value.starts_with('/') && !value.starts_with("//") {
                refs.push(value.split('?').next().unwrap_or(value).to_string());
            }
        }
    }
    refs
}

/// 抽出 ES 模块的静态与动态 import 说明符：`from "x"`、`import "x"`、`import("x")`。
fn import_specifiers(source: &str) -> Vec<String> {
    let mut specifiers = Vec::new();
    for marker in [
        "from \"",
        "import \"",
        "import(\"",
        "from '",
        "import '",
        "import('",
    ] {
        let quote = if marker.ends_with('"') { '"' } else { '\'' };
        let mut rest = source;
        while let Some(start) = rest.find(marker) {
            rest = &rest[start + marker.len()..];
            let end = rest.find(quote).unwrap_or(rest.len());
            specifiers.push(rest[..end].to_string());
        }
    }
    specifiers
}

/// 按浏览器的规则把说明符解析成站内绝对路径；裸说明符与跨域地址返回 None。
fn resolve(from: &str, specifier: &str) -> Option<String> {
    if specifier.starts_with('/') && !specifier.starts_with("//") {
        return Some(specifier.to_string());
    }
    if !(specifier.starts_with("./") || specifier.starts_with("../")) {
        return None;
    }
    let mut parts = from.split('/').collect::<Vec<_>>();
    parts.pop();
    for segment in specifier.split('/') {
        match segment {
            "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

#[test]
fn index_references_resolve_to_embedded_assets() {
    let known = known_paths();
    let html = include_str!("../../../web/index.html");
    let missing = same_origin_refs(html)
        .into_iter()
        .filter(|path| !known.contains(path.as_str()))
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "index.html references paths that are not served: {missing:?}"
    );
}

#[test]
fn module_imports_resolve_to_embedded_assets() {
    let known = known_paths();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("web");
    let mut missing = Vec::new();
    for path in embedded_paths().filter(|path| path.ends_with(".js")) {
        let Ok(source) = std::fs::read_to_string(root.join(path.trim_start_matches('/'))) else {
            continue;
        };
        for specifier in import_specifiers(&source) {
            if let Some(target) = resolve(path, &specifier) {
                if !known.contains(target.as_str()) {
                    missing.push(format!("{path} imports {specifier}"));
                }
            }
        }
    }
    assert!(missing.is_empty(), "unresolved module imports: {missing:?}");
}

#[test]
fn index_scripts_and_styles_carry_the_build_id() {
    let html = versioned_index_for_test("B1");
    let unversioned = same_origin_refs(&html)
        .into_iter()
        .filter(|path| path.ends_with(".js") || path.ends_with(".css"))
        .filter(|path| *path != "/theme.css")
        .filter(|path| !html.contains(&format!("{path}?v=B1\"")))
        .collect::<Vec<_>>();
    assert!(
        unversioned.is_empty(),
        "scripts or styles without ?v=: {unversioned:?}"
    );
}

#[test]
fn import_resolution_follows_browser_rules() {
    assert_eq!(
        resolve("/features/chat/view.js", "../../core/api.js").as_deref(),
        Some("/core/api.js")
    );
    assert_eq!(
        resolve("/app.js", "./core/icons.js").as_deref(),
        Some("/core/icons.js")
    );
    assert_eq!(resolve("/app.js", "lit").as_deref(), None);
    assert_eq!(
        import_specifiers("import { a } from \"./a.js\";\nconst b = await import('./b.js');"),
        vec!["./a.js".to_string(), "./b.js".to_string()]
    );
}

#[tokio::test]
async fn scanned_assets_are_served_with_type_and_etag() {
    let router = crate::web::with_web_assets(axum::Router::new());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await });
    let client = reqwest::Client::new();
    let get = |path: &str| client.get(format!("http://{address}{path}")).send();

    for (path, content_type) in [
        ("/app.js", "application/javascript; charset=utf-8"),
        ("/dashboards.js", "application/javascript; charset=utf-8"),
        ("/styles.css", "text/css; charset=utf-8"),
        ("/assets/gqy-logo.png", "image/png"),
    ] {
        let response = get(path).await.unwrap();
        assert_eq!(response.status(), 200, "{path}");
        assert_eq!(response.headers()["content-type"], content_type, "{path}");
        assert!(response.headers().contains_key("etag"), "{path}");
    }
    // 旧地址与不在表里的路径一律 404，不被当成静态资源吞掉。
    for path in ["/dash/dashboards.js", "/README.md", "/api/nope"] {
        assert_eq!(get(path).await.unwrap().status(), 404, "{path}");
    }
    server.abort();
}
