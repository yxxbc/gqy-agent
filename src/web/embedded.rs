//! 编译进二进制的前端资源：`web/` 下的文件由 build.rs 扫描成一张表。
//!
//! 加一个前端文件 = 把它放进 `web/`，不用再改这里。例外只有三类，各有自己的
//! handler：`index.html`（引用要加版本号）、`fence-frame.html`（沙箱 CSP）、
//! `vendor/`（gzip 原样发出与 CORS 预检）。
//! `css/` 不逐个提供：build.rs 按文件名顺序拼成一份 `/styles.css`，文件顺序就是层叠顺序。
//!
//! 开发期可以用 `GQY_WEB_DIR` 让 daemon 现读仓库目录（只在 debug 构建，见 dev_assets.rs）；
//! 下面几个 `dev_*` 函数在发布版里恒为 None，那条路径不存在。
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
                if let Some(response) = dev_asset(&headers, path) {
                    return response;
                }
                embedded_asset(&headers, content, content_type)
            }),
        );
    }
    attach_dev_fallback(router)
}

#[cfg(debug_assertions)]
fn dev_asset(headers: &HeaderMap, path: &str) -> Option<Response> {
    crate::web::dev_assets::asset(headers, path)
}

#[cfg(not(debug_assertions))]
fn dev_asset(_: &HeaderMap, _: &str) -> Option<Response> {
    None
}

#[cfg(debug_assertions)]
fn attach_dev_fallback<S>(router: Router<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    crate::web::dev_assets::attach(router)
}

#[cfg(not(debug_assertions))]
fn attach_dev_fallback<S>(router: Router<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    if std::env::var_os("GQY_WEB_DIR").is_some() {
        tracing::warn!("GQY_WEB_DIR is ignored: release builds always serve the embedded WebUI");
    }
    router
}

/// 沙箱宿主页正文：开发期现读目录，否则用嵌入的那份。
pub(in crate::web) fn fence_frame_html() -> std::borrow::Cow<'static, str> {
    match dev_fence_frame() {
        Some(html) => std::borrow::Cow::Owned(html),
        None => std::borrow::Cow::Borrowed(FENCE_FRAME_HTML),
    }
}

/// 前端资源从哪来，给 /api/health 报告：`embedded`，或开发期的 `dir:<路径>`。
pub(in crate::web) fn web_assets_source() -> String {
    match dev_dir() {
        Some(dir) => format!("dir:{}", dir.display()),
        None => "embedded".to_string(),
    }
}

// 开发期目录的几个入口：debug 构建转给 dev_assets，发布构建恒为 None。
#[cfg(debug_assertions)]
fn dev_index(headers: &HeaderMap) -> Option<Response> {
    crate::web::dev_assets::index(headers)
}

#[cfg(not(debug_assertions))]
fn dev_index(_: &HeaderMap) -> Option<Response> {
    None
}

#[cfg(debug_assertions)]
fn dev_fence_frame() -> Option<String> {
    crate::web::dev_assets::fence_frame()
}

#[cfg(not(debug_assertions))]
fn dev_fence_frame() -> Option<String> {
    None
}

#[cfg(debug_assertions)]
fn dev_dir() -> Option<&'static PathBuf> {
    crate::web::dev_assets::dir()
}

#[cfg(not(debug_assertions))]
fn dev_dir() -> Option<&'static PathBuf> {
    None
}

pub(in crate::web) async fn index_asset(headers: HeaderMap) -> Response {
    if let Some(response) = dev_index(&headers) {
        return response;
    }
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
pub(in crate::web) fn versioned_index(html: &str, build_id: &str) -> String {
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
