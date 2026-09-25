//! WebUI 主题库：`~/.gqy/config/webui-themes/<名字>.css`。
//!
//! 主题只有 CSS（覆盖 `:root` 与 `body[data-theme="linen"]` 上的颜色、字体、圆角等
//! token），在默认样式之后加载。她用内置技能 `webui-theme` 写主题文件，用户在
//! 设置 → 界面 →「配色方案」里挑。前端 JS 不开放给她改：WebUI 能跑工具、改配置，
//! 谁能写前端脚本谁就能在管理员浏览器里执行代码（docs/design/2026-09-25-webui-isolation.md §4）。
//! CSS 执行不了代码；CSP `style-src 'self'` 也挡住了外链样式与字体。

use crate::web::*;

const THEME_DIR: &str = "webui-themes";
const MAX_THEME_BYTES: u64 = 256 * 1024;

fn theme_dir(state: &DaemonState) -> std::path::PathBuf {
    state.paths.config_dir.join(THEME_DIR)
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 40
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.starts_with('-')
}

/// 文件开头注释里的 `title:` / `description:`（也认「标题」「说明」），以及
/// 主色 `--md-sys-color-primary` 的第一个取值，给配色方案里的色块用。
fn describe(css: &str, name: &str) -> Value {
    let header = css
        .trim_start()
        .strip_prefix("/*")
        .and_then(|rest| rest.split("*/").next())
        .unwrap_or_default();
    let field = |keys: &[&str]| {
        header.lines().find_map(|line| {
            let line = line.trim().trim_start_matches('*').trim();
            keys.iter().find_map(|key| {
                line.strip_prefix(key)
                    .map(|rest| rest.trim_start_matches([':', '：']).trim().to_string())
                    .filter(|value| !value.is_empty())
            })
        })
    };
    let accent = css
        .split("--md-sys-color-primary:")
        .nth(1)
        .and_then(|rest| {
            let value = rest.split(';').next()?.trim();
            value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "#(),.% ".contains(c))
                .then(|| value.to_string())
        });
    json!({
        "name": name,
        "title": field(&["title", "标题"]).unwrap_or_else(|| name.to_string()),
        "description": field(&["description", "说明"]).unwrap_or_default(),
        "accent": accent,
    })
}

pub(in crate::web) async fn webui_themes_list(
    State(state): State<DaemonState>,
    headers: HeaderMap,
) -> std::result::Result<Json<Value>, ApiError> {
    require_identity(&headers, &state)?;
    let mut themes = Vec::new();
    if let Ok(mut entries) = tokio::fs::read_dir(theme_dir(&state)).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let Some(name) = path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_suffix(".css"))
                .filter(|name| valid_name(name))
            else {
                continue;
            };
            let Ok(metadata) = entry.metadata().await else {
                continue;
            };
            if !metadata.is_file() || metadata.len() > MAX_THEME_BYTES {
                continue;
            }
            if let Ok(css) = tokio::fs::read_to_string(&path).await {
                themes.push(describe(&css, name));
            }
        }
    }
    themes.sort_by(|a, b| a["title"].as_str().cmp(&b["title"].as_str()));
    let matugen = tokio::fs::metadata(state.paths.config_dir.join("webui-theme.css"))
        .await
        .is_ok();
    Ok(Json(
        json!({ "ok": true, "themes": themes, "matugen": matugen }),
    ))
}

/// 与 `/theme.css` 一样不要求登录：登录页也要按选中的主题上色，内容只是配色。
pub(in crate::web) async fn webui_theme_file(
    State(state): State<DaemonState>,
    Path(file): Path<String>,
) -> Response {
    let Some(name) = file.strip_suffix(".css").filter(|name| valid_name(name)) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let path = theme_dir(&state).join(format!("{name}.css"));
    match tokio::fs::metadata(&path).await {
        Ok(metadata) if metadata.is_file() && metadata.len() <= MAX_THEME_BYTES => {}
        _ => return StatusCode::NOT_FOUND.into_response(),
    }
    match tokio::fs::read(&path).await {
        Ok(bytes) => finish_asset_response(bytes.into_response(), "text/css; charset=utf-8"),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

pub(in crate::web) async fn webui_theme_delete(
    State(state): State<DaemonState>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> std::result::Result<Json<Value>, ApiError> {
    require_admin_mutation(&headers, &state)?;
    if !valid_name(&name) {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "invalid theme name"));
    }
    tokio::fs::remove_file(theme_dir(&state).join(format!("{name}.css")))
        .await
        .map_err(|error| ApiError::new(StatusCode::NOT_FOUND, error.to_string()))?;
    Ok(Json(json!({ "ok": true })))
}
