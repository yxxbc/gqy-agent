//! 直接问供应商自己的 `/models` 接口。
//!
//! models.dev 覆盖不到自建端点，但很多供应商自己会报 context window。这条路是
//! 补充：拿到了就用，拿不到不影响主流程。
//!
//! `fetch_models` 是配置界面（TUI、WebUI、OOBE）共用的「列模型」入口：HTTP
//! 供应商问 `/models`，内置 CLI 供应商问 CLI（`cli_catalog.rs`）。

use super::cli_catalog::builtin_cli_catalog;
use crate::models_cache::*;

pub(crate) static PROVIDER_API_CACHE: OnceLock<Mutex<HashMap<(String, String), u64>>> =
    OnceLock::new();

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ProviderApiCacheEntry {
    pub(crate) provider_id: String,
    pub(crate) model: String,
    pub(crate) context_window: u64,
}

pub fn spawn_provider_api_refresh(providers: Vec<crate::config::ProviderConfig>) {
    std::thread::spawn(move || {
        let mut discovered = Vec::new();
        for provider in providers {
            if let Ok(entries) = fetch_provider_context_windows(&provider) {
                discovered.extend(entries);
            }
        }
        if discovered.is_empty() {
            return;
        }
        let mut cache = provider_api_cache_lock().lock().unwrap();
        for entry in discovered {
            cache.insert((entry.provider_id, entry.model), entry.context_window);
        }
    });
}

pub(crate) fn fetch_provider_context_windows(
    provider: &crate::config::ProviderConfig,
) -> Result<Vec<ProviderApiCacheEntry>> {
    let mut api_key = provider.api_key.as_deref().unwrap_or_default();
    if api_key.is_empty() {
        return Ok(Vec::new());
    }
    let resolved_key;
    if let Some(env_name) = api_key.strip_prefix("$env:") {
        resolved_key = std::env::var(env_name).unwrap_or_default();
        api_key = &resolved_key;
    }
    if api_key.is_empty() {
        return Ok(Vec::new());
    }
    let url = provider_models_url(&provider.base_url);
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(provider.timeout_seconds.min(5)))
        .build()?;
    let mut request = client
        .get(url)
        .header("Accept", "application/json")
        .header("User-Agent", "gqy-model-metadata");
    if !api_key.is_empty() {
        request = request.bearer_auth(api_key);
    }
    let value = request.send()?.error_for_status()?.json::<Value>()?;
    let Some(models) = value.get("data").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    Ok(models
        .iter()
        .filter_map(|model| {
            let id = model.get("id")?.as_str()?.trim();
            let context_window = api_context_window(model)?;
            (!id.is_empty() && context_window > 0).then(|| ProviderApiCacheEntry {
                provider_id: provider.id.clone(),
                model: id.to_string(),
                context_window,
            })
        })
        .collect())
}

/// `cli_binary`:内置 CLI 供应商列模型要跑的二进制(见 `cli_catalog.rs`);
/// HTTP 供应商忽略。`config` 供 cline 的目录读 `plugins.cline.provider`。
pub(crate) fn fetch_models(
    config: &crate::config::AppConfig,
    provider: &crate::config::ProviderConfig,
    cli_binary: Option<&str>,
) -> Result<Vec<String>> {
    if provider.is_builtin_cli_provider() {
        // 本机 CLI 后端没有 /models HTTP 端点:目录问 CLI 要(失败就报错),
        // 再并上配置里手工加的名字。只返回 `provider.models` 的话,用户一旦
        // 只激活一个模型,下次进来就只剩那一个可选(09-03)。
        return builtin_cli_catalog(config, provider, cli_binary);
    }
    let api_key = provider.api_key.as_deref().unwrap_or_default();
    let mut api_key = if let Some(env_name) = api_key.strip_prefix("$env:") {
        std::env::var(env_name).unwrap_or_default()
    } else {
        api_key.to_string()
    };
    if api_key.is_empty() && provider.is_opencode_zen() {
        api_key = "public".to_string();
    }
    let url = provider_models_url(&provider.base_url);
    let mut request = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(provider.timeout_seconds))
        .build()?
        .get(url)
        .header("Accept", "application/json")
        .header("User-Agent", "gqy-config");
    if !api_key.is_empty() {
        request = request.bearer_auth(api_key);
    }
    let response = request.send()?;
    let status = response.status();
    let body = response.text()?;
    if !status.is_success() {
        anyhow::bail!("{status}: {body}");
    }
    let parsed: ModelsListResponse = serde_json::from_str(&body)?;
    Ok(parsed
        .data
        .into_iter()
        .map(|model| model.id)
        .filter(|id| !id.is_empty())
        .collect())
}

#[derive(Deserialize)]
struct ModelsListResponse {
    data: Vec<ModelsListEntry>,
}

#[derive(Deserialize)]
struct ModelsListEntry {
    id: String,
}

pub(crate) fn provider_models_url(base_url: &str) -> String {
    let mut url = base_url.trim().trim_end_matches('/').to_string();
    if url.ends_with("/chat/completions") {
        url.truncate(url.len() - "/chat/completions".len());
    }
    if url.ends_with("/v1") {
        format!("{url}/models")
    } else {
        format!("{url}/v1/models")
    }
}

pub(crate) fn api_context_window(model: &Value) -> Option<u64> {
    for key in [
        "context_window",
        "context_length",
        "max_context_length",
        "max_input_tokens",
        "input_token_limit",
    ] {
        if let Some(value) = model.get(key).and_then(Value::as_u64).filter(|v| *v > 0) {
            return Some(value);
        }
    }
    for parent in ["limit", "limits"] {
        if let Some(value) = model
            .get(parent)
            .and_then(|value| value.get("context"))
            .and_then(Value::as_u64)
            .filter(|v| *v > 0)
        {
            return Some(value);
        }
    }
    None
}
