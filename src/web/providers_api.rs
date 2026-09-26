//! 供应商模型目录:WebUI「拉取模型列表」与「从目录补全」两个按钮的后端。
//!
//! 入参是**草稿**供应商(还没保存的 base_url / 协议也要能试),密钥留空时借用
//! 当前配置里同 id 供应商的密钥——前端拿到的密钥本来就是掩码。HTTP 供应商
//! 打 `/models`,内置 CLI 供应商问 CLI 要目录(同 TUI),再逐个附上 models.dev
//! 的上下文窗口 / 输入模态 / 价格,前端一键写进表单,不用手填 JSON。

use crate::web::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::web) struct ProviderModelsRequest {
    pub(in crate::web) provider: Value,
    /// true = 向供应商拉取目录;false = 只给 `models` 里的名字补目录元数据。
    #[serde(default)]
    pub(in crate::web) fetch: bool,
    #[serde(default)]
    pub(in crate::web) models: Vec<String>,
}

#[derive(Serialize)]
pub(in crate::web) struct ProviderModelsResponse {
    pub(in crate::web) source: &'static str,
    pub(in crate::web) models: Vec<crate::models_cache::ModelCatalogEntry>,
}

/// cline 供应商候选(id + 各家模型数):设置里「cline 供应商 id」输入框的候选。
///
/// cline 的订阅/额度区分就是不同的供应商 id(`cline`、`cline-pass`……),这份
/// 列表直接来自 cline 本体自带的 `@cline/llms`,与模型目录、`-P` 同源。拉不到
/// 就报错让用户看见(与模型目录同一口径,不悄悄给一份空列表)。
pub(in crate::web) async fn cline_provider_candidates(
    State(state): State<DaemonState>,
    headers: HeaderMap,
) -> std::result::Result<Response, ApiError> {
    require_admin(&headers, &state)?;
    let config = state.manager.lock().unwrap().config.clone();
    let candidates =
        tokio::task::spawn_blocking(move || crate::config_tui::cline_provider_candidates(&config))
            .await
            .map_err(ApiError::internal)?
            .map_err(ApiError::internal)?;
    let providers = candidates
        .into_iter()
        .map(|(id, count)| {
            let label = if count > 0 {
                format!("{id} · {count} 个模型")
            } else {
                id.clone()
            };
            serde_json::json!({ "value": id, "label": label })
        })
        .collect::<Vec<_>>();
    Ok(Json(serde_json::json!({ "providers": providers })).into_response())
}

pub(in crate::web) async fn provider_models(
    State(state): State<DaemonState>,
    headers: HeaderMap,
    Json(request): Json<ProviderModelsRequest>,
) -> std::result::Result<Json<ProviderModelsResponse>, ApiError> {
    require_admin(&headers, &state)?;
    let mut provider: ProviderConfig =
        serde_json::from_value(request.provider).map_err(|error| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                format!("invalid provider: {}", safe_error_message(error)),
            )
        })?;
    let (current, paths) = {
        let manager = state.manager.lock().unwrap();
        (manager.config.clone(), state.paths.clone())
    };
    if provider
        .api_key
        .as_deref()
        .map(str::trim)
        .is_none_or(str::is_empty)
    {
        provider.api_key = current
            .providers
            .iter()
            .find(|item| item.id == provider.id)
            .and_then(|item| item.api_key.clone());
    }
    if request.fetch && !provider.is_builtin_cli_provider() && provider.base_url.trim().is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "provider base_url is required to fetch models",
        ));
    }
    let fetch = request.fetch;
    let requested = request.models;
    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<ProviderModelsResponse> {
        let (source, ids) = if fetch {
            let cli_binary = crate::config_tui::builtin_cli_binary(&current, &provider);
            let ids = crate::config_tui::fetch_models(&current, &provider, cli_binary.as_deref())?;
            (
                if provider.is_builtin_cli_provider() {
                    "cli"
                } else {
                    "http"
                },
                ids,
            )
        } else {
            ("catalog", requested)
        };
        let ids: Vec<String> = ids
            .into_iter()
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty())
            .collect();
        let mut models =
            crate::models_cache::describe_models(&paths, &provider.id, &provider.base_url, &ids);
        // CLI 线的模型不在 models.dev 目录里:窗口用拉目录带回来的那份填上,
        // 前端「从目录补全」才拿得到数。
        for model in &mut models {
            if model.context_window.is_some_and(|window| window > 0) {
                continue;
            }
            if let Some(window) = crate::config_tui::remembered_window(&provider.id, &model.id) {
                model.context_window = Some(window as u64);
            }
        }
        Ok(ProviderModelsResponse { source, models })
    })
    .await
    .map_err(|error| ApiError::internal(anyhow::anyhow!(error)))?;
    result.map(Json).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("failed to list models: {}", safe_error_message(error)),
        )
    })
}
