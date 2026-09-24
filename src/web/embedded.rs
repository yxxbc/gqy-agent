//! 编译进二进制的前端资源：`web/` 下的文件由 build.rs 扫描成一张表。
//!
//! 加一个前端文件 = 把它放进 `web/`，不用再改这里。例外只有三类，各有自己的
//! handler：`index.html`（引用要加版本号）、`fence-frame.html`（沙箱 CSP）、
//! `vendor/`（gzip 原样发出与 CORS 预检）。
//! `css/` 不逐个提供：build.rs 按文件名顺序拼成一份 `/styles.css`，文件顺序就是层叠顺序。
//!
//! `web/assets/` 里的 logo 与壁纸是 `pics/` 原图的显示尺寸副本（原图解码要占
//! 30 MiB 显存去画两个缩略图），重新生成见 `test_scripts/gen_web_assets.py`。

use crate::web::*;

include!(concat!(env!("OUT_DIR"), "/web_assets.rs"));

/// 第三方库也挂版本号：它们不在扫描表里，但同样从 index.html 引用。
const VERSIONED_VENDOR: &[&str] = &[
    "/vendor/katex/katex.min.css",
    "/vendor/katex/katex.min.js",
    "/vendor/prism/prism.min.js",
];

/// 把扫描表里的每个文件注册成一条精确路由。不用 `/{*path}` 通配：那样未知的
/// `/api/...` 也会先落到这里，错误形状就变了。
pub(in crate::web) fn with_web_assets<S>(mut router: Router<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    for &(path, content, content_type) in WEB_ASSETS {
        router = router.route(
            path,
            get(move |headers: HeaderMap| async move {
                embedded_asset(&headers, content, content_type)
            }),
        );
    }
    router
}

pub(in crate::web) async fn index_asset(headers: HeaderMap) -> Response {
    static VERSIONED_INDEX: std::sync::LazyLock<String> =
        std::sync::LazyLock::new(|| versioned_index(INDEX_HTML, env!("GQY_BUILD_ID")));
    embedded_asset(
        &headers,
        VERSIONED_INDEX.as_bytes(),
        "text/html; charset=utf-8",
    )
}

/// 给 index.html 里每个同源资源引用挂上 `?v=构建号`，升级后浏览器和中间缓存
/// 不可能再拿旧文件。模块之间的 `import` 不经过这里，由 ETag + no-cache 兜底。
fn versioned_index(html: &str, build_id: &str) -> String {
    // 只挂脚本与样式：图片在 JS 里也按裸路径引用，挂了版本号反而一图两份缓存。
    let paths = WEB_ASSETS
        .iter()
        .map(|&(path, _, _)| path)
        .filter(|path| path.ends_with(".js") || path.ends_with(".css"))
        .chain(VERSIONED_VENDOR.iter().copied());
    let mut html = html.to_string();
    for path in paths {
        for attribute in ["src", "href"] {
            html = html.replace(
                &format!("{attribute}=\"{path}\""),
                &format!("{attribute}=\"{path}?v={build_id}\""),
            );
        }
    }
    html
}

/// 路由表里所有静态资源的路径，供测试核对引用。
#[cfg(test)]
pub(in crate::web) fn embedded_paths() -> impl Iterator<Item = &'static str> {
    WEB_ASSETS
        .iter()
        .map(|&(path, _, _)| path)
        .chain(VERSIONED_VENDOR.iter().copied())
}

#[cfg(test)]
pub(in crate::web) fn versioned_index_for_test(build_id: &str) -> String {
    versioned_index(INDEX_HTML, build_id)
}
