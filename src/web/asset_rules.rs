// `web/` 下哪些文件作为静态资源提供、以什么类型提供。
//
// 两处共用这一份规则：build.rs（`include!` 进去生成嵌入表）与开发期的运行时目录
// （`GQY_WEB_DIR`，见 embedded.rs）。所以这里只能用 std，不能引用本 crate 的其他东西。

/// 有专门 handler 的文件，不进通用表：index.html 要挂版本号，fence-frame.html 要专用 CSP。
pub(crate) const WEB_SPECIAL_FILES: &[&str] = &["index.html", "fence-frame.html"];

/// 扫描时跳过的目录：`vendor/` 手工提供（gzip 原样发出与 CORS 预检），
/// `css/` 不逐个提供，而是拼成一份 `/styles.css`。
pub(crate) fn web_skip_dir(name: &std::ffi::OsStr) -> bool {
    name == "vendor" || name == "css"
}

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

/// `css/*.css` 按文件名顺序逐字节拼接：文件顺序就是层叠顺序。各段自带结尾换行，
/// 中间不插任何东西。
pub(crate) fn web_concat_css(dir: &std::path::Path) -> std::io::Result<Vec<u8>> {
    let mut parts = std::fs::read_dir(dir)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    parts.retain(|path| path.extension().is_some_and(|ext| ext == "css"));
    parts.sort();
    let mut out = Vec::new();
    for part in parts {
        out.extend(std::fs::read(&part)?);
    }
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
