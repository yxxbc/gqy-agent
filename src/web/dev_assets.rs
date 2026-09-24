//! 开发期运行时目录：设置了 `GQY_WEB_DIR` 时，静态资源每次请求现读该目录，
//! 前端改完刷新浏览器即生效，不用重编 gqy。
//!
//! **只在 debug 构建里存在**（mod.rs 里 `#[cfg(debug_assertions)]`），发布版没有这条路径。
//! 原因见 docs/design/2026-09-25-webui-isolation.md §4：WebUI 能跑工具、改配置，
//! 谁能写前端文件谁就能在管理员浏览器里执行代码，而顾清影自己有写文件的工具。
//! 所以也只认环境变量（要人在终端里设），不进配置文件。
//!
//! 哪些文件提供、以什么类型提供，和 build.rs 用的是同一份规则（asset_rules.rs）。

use crate::web::asset_rules::{web_concat_css, web_content_type, web_skip_dir, WEB_SPECIAL_FILES};
use crate::web::*;
use axum::http::header::{ETAG, IF_NONE_MATCH};
use axum::http::{Method, Uri};
use std::path::Component;

/// 规范化后的 `GQY_WEB_DIR`。目录里没有 index.html 就当没设，退回嵌入的那份。
pub(in crate::web) fn dir() -> Option<&'static PathBuf> {
    static DIR: std::sync::LazyLock<Option<PathBuf>> = std::sync::LazyLock::new(|| {
        let raw = std::env::var_os("GQY_WEB_DIR")?;
        match PathBuf::from(&raw).canonicalize() {
            Ok(dir) if dir.join("index.html").is_file() => {
                tracing::warn!(
                    dir = %dir.display(),
                    "WebUI assets are read from GQY_WEB_DIR on every request (debug build)"
                );
                Some(dir)
            }
            _ => {
                tracing::warn!(dir = ?raw, "GQY_WEB_DIR has no index.html; serving the embedded WebUI");
                None
            }
        }
    });
    DIR.as_ref()
}

/// URL 路径 → 目录里的文件与类型。越界、特殊文件、跳过的目录、不认识的类型一律 None。
fn resolve(dir: &FilePath, url_path: &str) -> Option<(PathBuf, &'static str)> {
    let relative = url_path.strip_prefix('/')?;
    if relative.is_empty() || WEB_SPECIAL_FILES.contains(&relative) {
        return None;
    }
    let path = FilePath::new(relative);
    let components = path.components().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(name) = component else {
            return None; // `..`、根、盘符：一律拒绝
        };
        if index + 1 < components.len() && web_skip_dir(name) {
            return None;
        }
    }
    let content_type = web_content_type(path)?;
    // 符号链接指到目录外也算越界：比较规范化之后的真实路径。
    let file = dir.join(path).canonicalize().ok()?;
    (file.starts_with(dir) && file.is_file()).then_some((file, content_type))
}

/// 与嵌入资源同样的缓存语义：no-cache + ETag。ETag 取内容哈希，改了文件就变。
fn respond(headers: &HeaderMap, body: Vec<u8>, content_type: &'static str) -> Response {
    let digest = Sha256::digest(&body);
    let hex = digest[..12]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let etag =
        HeaderValue::from_str(&format!("\"dev-{hex}\"")).expect("hex is a valid header value");
    if headers.get(IF_NONE_MATCH) == Some(&etag) {
        let mut response = StatusCode::NOT_MODIFIED.into_response();
        response.headers_mut().insert(ETAG, etag);
        return response;
    }
    let mut response = finish_asset_response(body.into_response(), content_type);
    response.headers_mut().insert(ETAG, etag);
    response
}

/// 目录模式下的静态资源：设了目录就一定给出响应（找不到是 404，不退回嵌入的旧文件）。
pub(in crate::web) fn asset(headers: &HeaderMap, url_path: &str) -> Option<Response> {
    let dir = dir()?;
    if url_path == "/styles.css" {
        return Some(match web_concat_css(&dir.join("css")) {
            Ok(body) => respond(headers, body, "text/css; charset=utf-8"),
            Err(_) => StatusCode::NOT_FOUND.into_response(),
        });
    }
    Some(
        match resolve(dir, url_path)
            .and_then(|(file, content_type)| Some((std::fs::read(file).ok()?, content_type)))
        {
            Some((body, content_type)) => respond(headers, body, content_type),
            None => StatusCode::NOT_FOUND.into_response(),
        },
    )
}

/// 目录模式下的 index.html：现读现改写。版本号固定为 `dev`，靠 ETag 保证拿到新文件。
pub(in crate::web) fn index(headers: &HeaderMap) -> Option<Response> {
    let dir = dir()?;
    let html = std::fs::read_to_string(dir.join("index.html")).ok()?;
    Some(respond(
        headers,
        super::embedded::versioned_index(&html, "dev").into_bytes(),
        "text/html; charset=utf-8",
    ))
}

/// 目录模式下的沙箱宿主页正文；CSP 仍由原 handler 加。
pub(in crate::web) fn fence_frame() -> Option<String> {
    std::fs::read_to_string(dir()?.join("fence-frame.html")).ok()
}

/// 目录里新加的文件不在编译期那张路由表里，由这个兜底提供。只接 GET/HEAD，
/// 其余情况与没有兜底时一样是 404。
pub(in crate::web) fn attach<S>(router: Router<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    if dir().is_none() {
        return router;
    }
    router.fallback(|method: Method, uri: Uri, headers: HeaderMap| async move {
        if method != Method::GET && method != Method::HEAD {
            return StatusCode::NOT_FOUND.into_response();
        }
        asset(&headers, uri.path()).unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
    })
}

#[cfg(test)]
mod tests {
    use super::resolve;
    use std::fs;

    fn sandbox() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("gqy-dev-assets-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for dir in ["web/core", "web/css", "web/vendor", "outside"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        for file in [
            "web/index.html",
            "web/app.js",
            "web/core/api.js",
            "web/css/00-tokens.css",
            "web/vendor/lib.js",
            "web/notes.md",
            "outside/secret.js",
        ] {
            fs::write(root.join(file), "x").unwrap();
        }
        root
    }

    #[test]
    fn resolves_only_servable_files_inside_the_directory() {
        let root = sandbox();
        let web = root.join("web").canonicalize().unwrap();
        assert!(resolve(&web, "/app.js").is_some());
        assert!(resolve(&web, "/core/api.js").is_some());
        for rejected in [
            "/",
            "/index.html",           // 有专门 handler
            "/css/00-tokens.css",    // 拼成 /styles.css，不逐个提供
            "/vendor/lib.js",        // 手工提供
            "/notes.md",             // 不认识的类型
            "/../outside/secret.js", // 越界
            "/core/../../outside/secret.js",
            "app.js", // 不以 / 开头
        ] {
            assert!(resolve(&web, rejected).is_none(), "{rejected}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("outside/secret.js"), web.join("link.js"))
                .unwrap();
            assert!(
                resolve(&web, "/link.js").is_none(),
                "symlink escaping the directory"
            );
        }
        let _ = fs::remove_dir_all(&root);
    }
}
