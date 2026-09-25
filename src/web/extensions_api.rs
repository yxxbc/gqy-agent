//! 设置 → 插件 →「扩展」分组：脚本工具、MCP 服务器、技能、pm 包的汇总与操作。
//!
//! 设计见 docs/plan/2026-09-24-extensions-in-plugin-list.md。这里只做聚合与转发：
//! - 脚本的开关与删除沿用控制台脚本面板的 `/api/dash/scripts/*`，前端直接调；
//! - MCP 的开关是改配置草稿（和内置插件卡片一样，点「保存配置」生效），这里只给状态；
//! - 技能与 pm 包的写操作在下面，逻辑在 `skills::admin` 与 `pm::upgrade`。
//!
//! 技能目录与脚本目录每一轮开头按指纹重扫，改完下一轮就生效，不用重载 daemon。

use crate::config::AppConfig;
use crate::paths::GqyPaths;
use crate::web::*;
use anyhow::Context;

fn active(state: &DaemonState) -> (AppConfig, GqyPaths) {
    let config = state.manager.lock().unwrap().config.clone();
    (config, state.paths.clone())
}

fn bad_request(error: anyhow::Error) -> ApiError {
    ApiError::new(StatusCode::BAD_REQUEST, safe_error_message(&error))
}

async fn blocking<T, F>(work: F) -> std::result::Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce() -> anyhow::Result<T> + Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(ApiError::internal)?
        .map_err(bad_request)
}

/// 包里装进来的文件（相对 GQY 根目录）→ 包名，用来给脚本和技能标「来自哪个包」。
fn package_owner(paths: &GqyPaths, lock: &crate::pm::LockFile, absolute: &str) -> Option<String> {
    let relative = std::path::Path::new(absolute)
        .strip_prefix(&paths.root_dir)
        .ok()?
        .to_string_lossy()
        .to_string();
    lock.packages.iter().find_map(|(name, package)| {
        package
            .files
            .iter()
            .any(|file| file == &relative || file.starts_with(&format!("{relative}/")))
            .then(|| name.clone())
    })
}

/// MCP 服务器落在哪个文件上：参数里第一个存在的绝对路径（server.py、server.mjs），
/// 没有就用命令本身（.venv 里的可执行文件）。`npx 包名` 这类纯命令没有落点。
fn mcp_path(server: &crate::config::McpServerConfig) -> Option<std::path::PathBuf> {
    server
        .args
        .iter()
        .chain(std::iter::once(&server.command))
        .map(std::path::PathBuf::from)
        .find(|path| path.is_absolute() && path.exists())
}

fn mcp_origin(server: &crate::config::McpServerConfig, paths: &GqyPaths) -> crate::pm::Origin {
    match mcp_path(server) {
        Some(path) if !is_system_interpreter(&path) => {
            crate::pm::detect_origin(&path, &paths.root_dir)
        }
        _ => crate::pm::Origin {
            kind: "managed",
            dir: None,
            remote: None,
            commit: None,
            deps: Vec::new(),
        },
    }
}

/// /usr/bin/python3、/opt/homebrew/bin/node 这类解释器不是扩展本身。
fn is_system_interpreter(path: &std::path::Path) -> bool {
    [
        "/usr/bin/",
        "/bin/",
        "/usr/local/bin/",
        "/opt/homebrew/bin/",
        "/opt/homebrew/opt/",
    ]
    .iter()
    .any(|prefix| path.starts_with(prefix))
}

/// 能检查更新 / 更新的目录：只认当前识别出来的 git 来源，不接受任意路径。
fn known_git_dir(
    config: &AppConfig,
    paths: &GqyPaths,
    dir: &str,
) -> anyhow::Result<std::path::PathBuf> {
    let from_mcp = config
        .mcp
        .servers
        .iter()
        .map(|server| mcp_origin(server, paths));
    let from_skills = crate::skills::admin_catalog(config, paths)?
        .into_iter()
        .filter_map(|skill| skill.path)
        .map(|path| crate::pm::detect_origin(std::path::Path::new(&path), &paths.root_dir));
    from_mcp
        .chain(from_skills)
        .filter(|origin| origin.kind == "git")
        .find_map(|origin| origin.dir.filter(|known| known == dir))
        .map(std::path::PathBuf::from)
        .with_context(|| format!("{dir} is not a known extension repository"))
}

fn collect(config: &AppConfig, paths: &GqyPaths) -> anyhow::Result<Value> {
    let lock = crate::pm::load_lock(paths).unwrap_or_default();

    let overview = crate::tools::scripts_dashboard_overview(config, paths)?;
    let mut scripts = Vec::new();
    for script in overview["scripts"].as_array().into_iter().flatten() {
        let path = script["path"].as_str().unwrap_or_default();
        scripts.push(json!({
            "id": script["id"],
            "title": script["display_name"],
            "description": script["description"],
            "enabled": true,
            "builtin": script["builtin"],
            "layer": script["layer"],
            "path": path,
            "parameters": script["parameter_names"],
            "package": package_owner(paths, &lock, path),
        }));
    }
    for script in overview["disabled"].as_array().into_iter().flatten() {
        let path = script["path"].as_str().unwrap_or_default();
        scripts.push(json!({
            "id": script["id"],
            "title": script["id"],
            "description": "",
            "enabled": false,
            "builtin": script["builtin"],
            "layer": script["scope"],
            "path": path,
            "parameters": [],
            "package": package_owner(paths, &lock, path),
        }));
    }

    let mcp: Vec<Value> = config
        .mcp
        .servers
        .iter()
        .enumerate()
        .map(|(index, server)| {
            let (status, tools, error) = match crate::tools::mcp_listing_status(server) {
                None => ("unknown", Vec::new(), None),
                Some(Ok(tools)) => ("ok", tools, None),
                Some(Err(error)) => ("failed", Vec::new(), Some(error)),
            };
            json!({
                "index": index,
                "id": server.id,
                "title": if server.display_name.trim().is_empty() { &server.id } else { &server.display_name },
                "command": server.command,
                "args": server.args,
                "enabled": server.enabled,
                "status": status,
                "tools": tools.iter().map(|(name, description)| json!({"name": name, "description": description})).collect::<Vec<_>>(),
                "error": error,
                "origin": mcp_origin(server, paths),
            })
        })
        .collect();

    let skills: Vec<Value> = crate::skills::admin_catalog(config, paths)?
        .into_iter()
        .map(|skill| {
            let package = skill
                .path
                .as_deref()
                .and_then(|path| package_owner(paths, &lock, path));
            let origin = match (&package, skill.path.as_deref()) {
                (Some(_), _) => json!({ "kind": "pm" }),
                (None, Some(path)) => json!(crate::pm::detect_origin(
                    std::path::Path::new(path),
                    &paths.root_dir
                )),
                (None, None) => json!({ "kind": "builtin" }),
            };
            let mut value = serde_json::to_value(&skill).unwrap_or_default();
            value["package"] = json!(package);
            value["origin"] = origin;
            value
        })
        .collect();

    let packages: Vec<Value> = lock
        .packages
        .iter()
        .map(|(name, package)| {
            json!({
                "name": name,
                "version": package.version,
                "description": package.description,
                "source": package.source,
                "reference": package.reference,
                "commit": package.commit,
                "kind": package.kind,
                "installed_at": package.installed_at,
                "files": package.files,
            })
        })
        .collect();

    Ok(json!({
        "ok": true,
        "mcp_enabled": config.mcp.enabled,
        "skills_enabled": config.skills.enabled,
        "scripts": scripts,
        "mcp": mcp,
        "skills": skills,
        "packages": packages,
    }))
}

pub(in crate::web) async fn extensions_overview(
    State(state): State<DaemonState>,
    headers: HeaderMap,
) -> std::result::Result<Json<Value>, ApiError> {
    require_admin(&headers, &state)?;
    let (config, paths) = active(&state);
    Ok(Json(blocking(move || collect(&config, &paths)).await?))
}

#[derive(Deserialize)]
pub(in crate::web) struct SkillToggle {
    name: String,
    enabled: bool,
}

pub(in crate::web) async fn extensions_skill_toggle(
    State(state): State<DaemonState>,
    headers: HeaderMap,
    Json(body): Json<SkillToggle>,
) -> std::result::Result<Json<Value>, ApiError> {
    require_admin_mutation(&headers, &state)?;
    let (config, paths) = active(&state);
    blocking(move || crate::skills::set_skill_enabled(&config, &paths, &body.name, body.enabled))
        .await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub(in crate::web) struct SkillCreate {
    name: String,
    description: String,
    #[serde(default)]
    body: String,
}

pub(in crate::web) async fn extensions_skill_create(
    State(state): State<DaemonState>,
    headers: HeaderMap,
    Json(body): Json<SkillCreate>,
) -> std::result::Result<Json<Value>, ApiError> {
    require_admin_mutation(&headers, &state)?;
    let (config, paths) = active(&state);
    let published = blocking(move || {
        crate::skills::create_skill(
            &config,
            &paths,
            body.name.trim(),
            &body.description,
            &body.body,
        )
    })
    .await?;
    Ok(Json(json!({ "ok": true, "skill": published })))
}

#[derive(Deserialize)]
pub(in crate::web) struct NameQuery {
    name: String,
}

pub(in crate::web) async fn extensions_skill_source(
    State(state): State<DaemonState>,
    headers: HeaderMap,
    Query(query): Query<NameQuery>,
) -> std::result::Result<Json<Value>, ApiError> {
    require_admin(&headers, &state)?;
    let (config, paths) = active(&state);
    let text =
        blocking(move || crate::skills::skill_source_text(&config, &paths, &query.name)).await?;
    Ok(Json(json!({ "ok": true, "source": text })))
}

pub(in crate::web) async fn extensions_skill_delete(
    State(state): State<DaemonState>,
    headers: HeaderMap,
    Query(query): Query<NameQuery>,
) -> std::result::Result<Json<Value>, ApiError> {
    require_admin_mutation(&headers, &state)?;
    let (config, paths) = active(&state);
    blocking(move || crate::skills::remove_skill(&config, &paths, &query.name)).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub(in crate::web) struct PackageName {
    name: String,
}

/// 升级按钮本身就是确认：准备好就直接装。已是最新返回 `updated: false`。
pub(in crate::web) async fn extensions_package_upgrade(
    State(state): State<DaemonState>,
    headers: HeaderMap,
    Json(body): Json<PackageName>,
) -> std::result::Result<Json<Value>, ApiError> {
    require_admin_mutation(&headers, &state)?;
    let (config, paths) = active(&state);
    let prepared = crate::pm::prepare_upgrade(&config, &paths, &body.name)
        .await
        .map_err(bad_request)?;
    let Some(prepared) = prepared else {
        return Ok(Json(json!({ "ok": true, "updated": false })));
    };
    let (from, to) = (prepared.from_version.clone(), prepared.to_version.clone());
    blocking(move || crate::pm::apply_upgrade(&paths, prepared)).await?;
    Ok(Json(
        json!({ "ok": true, "updated": true, "from": from, "to": to }),
    ))
}

pub(in crate::web) async fn extensions_package_remove(
    State(state): State<DaemonState>,
    headers: HeaderMap,
    Query(query): Query<NameQuery>,
) -> std::result::Result<Json<Value>, ApiError> {
    require_admin_mutation(&headers, &state)?;
    let (_, paths) = active(&state);
    let removed = blocking(move || crate::pm::remove(&paths, &query.name)).await?;
    Ok(Json(json!({ "ok": true, "files": removed.files.len() })))
}

#[derive(Deserialize)]
pub(in crate::web) struct OriginDir {
    dir: String,
}

pub(in crate::web) async fn extensions_origin_check(
    State(state): State<DaemonState>,
    headers: HeaderMap,
    Json(body): Json<OriginDir>,
) -> std::result::Result<Json<Value>, ApiError> {
    require_admin(&headers, &state)?;
    let (config, paths) = active(&state);
    let dir = blocking(move || known_git_dir(&config, &paths, &body.dir)).await?;
    let check = crate::pm::check_update(&dir).await.map_err(bad_request)?;
    Ok(Json(json!({ "ok": true, "check": check })))
}

/// 拉到远端最新提交并同步依赖（npm / uv / pip）。已跟踪文件有本地修改时拒绝。
pub(in crate::web) async fn extensions_origin_update(
    State(state): State<DaemonState>,
    headers: HeaderMap,
    Json(body): Json<OriginDir>,
) -> std::result::Result<Json<Value>, ApiError> {
    require_admin_mutation(&headers, &state)?;
    let (config, paths) = active(&state);
    let dir = blocking(move || known_git_dir(&config, &paths, &body.dir)).await?;
    let report = crate::pm::update(&dir).await.map_err(bad_request)?;
    Ok(Json(json!({ "ok": true, "report": report })))
}
