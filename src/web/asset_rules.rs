// `web/` 下哪些文件作为静态资源提供、以什么类型提供。
//
// 两处共用这一份规则：build.rs（`include!` 进去生成嵌入表）与开发期的运行时目录
// （`GQY_WEB_DIR`，见 embedded.rs）。所以这里只能用 std，不能引用本 crate 的其他东西。

/// 有专门 handler 的文件，不进通用表：index.html 要挂版本号，fence-frame.html 要专用 CSP。
pub(crate) const WEB_SPECIAL_FILES: &[&str] = &["index.html", "fence-frame.html"];

/// 扫描时跳过的目录：`vendor/` 手工提供（gzip 原样发出与 CORS 预检），
/// `css/` 与 `settings-schema/` 不逐个提供，而是各自拼成一份（见下）。
pub(crate) fn web_skip_dir(name: &std::ffi::OsStr) -> bool {
    name == "vendor" || name == "css" || name == WEB_SETTINGS_SCHEMA_DIR
}

/// 设置页字段表的分段目录，拼成一份 `/settings-schema.js` 提供。
pub(crate) const WEB_SETTINGS_SCHEMA_DIR: &str = "settings-schema";

/// 递归收集 `dir` 下的文件（跳过 [`web_skip_dir`] 的目录）。
pub(crate) fn web_collect_files(
    dir: &std::path::Path,
    files: &mut Vec<std::path::PathBuf>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| !web_skip_dir(name)) {
                web_collect_files(&path, files)?;
            }
        } else {
            files.push(path);
        }
    }
    Ok(())
}

/// `dir` 下扩展名为 `ext` 的文件按文件名顺序逐字节拼接。各段自带结尾换行，
/// 中间不插任何东西。
fn web_concat_parts(dir: &std::path::Path, ext: &str) -> std::io::Result<Vec<u8>> {
    let mut parts = std::fs::read_dir(dir)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    parts.retain(|path| path.extension().is_some_and(|found| found == ext));
    parts.sort();
    let mut out = Vec::new();
    for part in parts {
        out.extend(std::fs::read(&part)?);
    }
    Ok(out)
}

/// `css/*.css` 拼成 `/styles.css`：文件顺序就是层叠顺序。
pub(crate) fn web_concat_css(dir: &std::path::Path) -> std::io::Result<Vec<u8>> {
    web_concat_parts(dir, "css")
}

/// `settings-schema/*.js` 拼成 `/settings-schema.js`，外面包一层 IIFE。各段只写
/// 顶层 `const`，拼进同一个函数作用域后互相可见，又不漏成全局变量。
/// `src/web/tests/settings_schema.rs` 读的也是拼好的这一份。
pub(crate) fn web_concat_settings_schema(dir: &std::path::Path) -> std::io::Result<Vec<u8>> {
    let mut out = b"// Generated from web/settings-schema/*.js; edit the parts, not this file.\n\
\"use strict\";\n\n(function () {\n"
        .to_vec();
    out.extend(web_concat_parts(dir, "js")?);
    out.extend_from_slice(b"})();\n");
    Ok(out)
}

/// 按扩展名定类型；不认识的扩展名返回 None，那个文件就不提供。
pub(crate) fn web_content_type(path: &std::path::Path) -> Option<&'static str> {
    Some(match path.extension()?.to_str()? {
        "js" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "html" => "text/html; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "webp" => "image/webp",
        "woff2" => "font/woff2",
        _ => return None,
    })
}
