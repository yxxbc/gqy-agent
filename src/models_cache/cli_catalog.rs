//! 内置 CLI 供应商的模型目录。
//!
//! 它们没有 /models HTTP 端点,但 CLI 自己能列:`agy models`(TSV,
//! `slug<TAB>显示名`)、`codex debug models`(JSON,`models[].slug`,
//! `visibility: hide` 的不列)、cline 本体自带的 `@cline/llms`
//! (`getModelsForProvider`,与 cline TUI 的模型选择器同源,经 node 打印成
//! JSON)。目录就问 CLI 要;CLI 不在、超时或输出不认识一律报错让用户看见
//! (09-03 裁定:不悄悄退回快照——预置表只在首次创建供应商时当模板用)。
//! 配置里手工加的名字并进目录,去重保序。`claude` 没有列模型的子命令
//! (`--model` 只认 fable/opus/sonnet/haiku 别名或完整名),它的目录就是那张
//! 别名表。

use crate::config::{AppConfig, ProviderConfig};
use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// agy 要联网取目录,cline 要起 node 读包,给足时间;超时就不让 TUI 干等。
const CLI_LIST_TIMEOUT: Duration = Duration::from_secs(20);

/// 目录里的一条:模型名 + 已知的上下文窗口(多数 CLI 不给)。
struct LiveModel {
    id: String,
    context_window: Option<u64>,
}

/// CLI 目录带回的上下文窗口(进程内,键 = 顾清影 供应商 id + 模型名)。
///
/// CLI 线的模型不在 models.dev 目录里,激活时 `auto_configure_model_tags` 与
/// WebUI 的目录补全都查不到窗口;拉目录时顺手把窗口记在这里,之后只查内存,
/// 不再起一次 CLI。键带供应商 id:同名模型在两家可以是两个窗口。
static CLI_WINDOWS: LazyLock<Mutex<HashMap<(String, String), usize>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// cline 供应商候选的缓存(键 = 解析后的二进制路径):226 家逐个数一次模型要起
/// 一次 node,输入框聚焦一次就拉一次,缓存住别反复起。
static CLINE_PROVIDERS: LazyLock<Mutex<Option<(String, Vec<(String, u32)>)>>> =
    LazyLock::new(|| Mutex::new(None));

fn remember_windows(provider_id: &str, models: &[LiveModel]) {
    let Ok(mut cache) = CLI_WINDOWS.lock() else {
        return;
    };
    for model in models {
        if let Some(window) = model.context_window.filter(|window| *window > 0) {
            cache.insert((provider_id.to_string(), model.id.clone()), window as usize);
        }
    }
}

/// 拉过目录的模型窗口;没拉过就是 `None`(调用方什么都不做)。
pub(crate) fn remembered_window(provider_id: &str, model: &str) -> Option<usize> {
    CLI_WINDOWS
        .lock()
        .ok()?
        .get(&(provider_id.to_string(), model.to_string()))
        .copied()
}

/// 该供应商列模型要跑的二进制;`None` = 没有对应的 CLI 子命令。
pub(crate) fn builtin_cli_binary(config: &AppConfig, provider: &ProviderConfig) -> Option<String> {
    let pick = |configured: &str, fallback: &str| {
        let configured = configured.trim();
        if configured.is_empty() {
            fallback.to_string()
        } else {
            configured.to_string()
        }
    };
    if provider.is_antigravity() {
        Some(pick(&config.plugins.antigravity.binary, "agy"))
    } else if provider.is_codex() {
        Some(pick(&config.plugins.codex.binary, "codex"))
    } else if provider.is_cline() {
        Some(pick(&config.plugins.cline.binary, "cline"))
    } else {
        None
    }
}

/// 目录 = CLI 实时列表(claude:别名表)∪ 配置里手工加的。CLI 失败即失败。
pub(in crate::models_cache) fn builtin_cli_catalog(
    config: &AppConfig,
    provider: &ProviderConfig,
    binary: Option<&str>,
) -> Result<Vec<String>> {
    let mut catalog = match binary {
        Some(binary) => {
            let models = live_catalog(config, provider, binary)?;
            remember_windows(&provider.id, &models);
            let ids: Vec<String> = models.into_iter().map(|model| model.id).collect();
            if ids.is_empty() {
                bail!("{binary} listed no models");
            }
            ids
        }
        None => provider
            .preset_model_catalog()
            .iter()
            .map(|name| name.to_string())
            .collect(),
    };
    for name in &provider.models {
        if !catalog.iter().any(|known| known == name) {
            catalog.push(name.clone());
        }
    }
    Ok(catalog)
}

fn live_catalog(
    config: &AppConfig,
    provider: &ProviderConfig,
    binary: &str,
) -> Result<Vec<LiveModel>> {
    if provider.is_antigravity() {
        let stdout = run_with_timeout(binary, &["models"], CLI_LIST_TIMEOUT)?;
        Ok(parse_agy_models(&stdout)
            .into_iter()
            .map(|id| LiveModel {
                id,
                context_window: None,
            })
            .collect())
    } else if provider.is_codex() {
        let stdout = run_with_timeout(binary, &["debug", "models"], CLI_LIST_TIMEOUT)?;
        Ok(parse_codex_models(&stdout)?
            .into_iter()
            .map(|id| LiveModel {
                id,
                context_window: None,
            })
            .collect())
    } else if provider.is_cline() {
        cline_catalog(config, binary)
    } else {
        bail!("this CLI has no model listing command")
    }
}

/// cline 的模型目录:读 cline 本体自带的 `@cline/llms`(与 cline TUI 的模型
/// 选择器同源),不连 Cline 的服务器。定位方式:把 cline 可执行文件解掉符号
/// 链接,从它的祖先目录里找 `node_modules/@cline/llms`,再让 node 跑一段小
/// 脚本把 `getModelsForProvider(<供应商 id>)` 的条目打成 JSON(带
/// `contextWindow`,激活时用来填窗口)。找不到 node 或包就报错让用户看见
/// (与另两条线一致:目录问题不悄悄降级)。
fn cline_catalog(config: &AppConfig, binary: &str) -> Result<Vec<LiveModel>> {
    let provider_id = {
        let configured = config.plugins.cline.provider.trim();
        if configured.is_empty() {
            "cline"
        } else {
            configured
        }
    };
    let resolved = resolve_binary_path(binary)?;
    let entry = locate_cline_llms(&resolved).with_context(|| {
        format!(
            "cline's bundled @cline/llms was not found above {}; the install may be incomplete",
            resolved.display()
        )
    })?;
    let script = format!(
        "const m = await import({});\n\
         const models = await m.getModelsForProvider({});\n\
         process.stdout.write(JSON.stringify(Object.values(models ?? {{}}).map((model) => ({{ id: model.id, contextWindow: model.contextWindow }}))));",
        serde_json::to_string(&entry.display().to_string())?,
        serde_json::to_string(provider_id)?,
    );
    let stdout = run_with_timeout(
        "node",
        &["--input-type=module", "-e", &script],
        CLI_LIST_TIMEOUT,
    )
    .context("running node for the cline model catalog")?;
    parse_cline_models(&stdout)
}

/// cline 的供应商候选(id + 模型数):设置里「cline 供应商 id」输入框的候选,
/// 与模型目录、`-P` 同一个数据源(`getProviderIds()` 逐家数模型)。
///
/// cline 的订阅/额度区分就是不同的供应商 id(`cline` 账号额度、`cline-pass`
/// 订阅……),这里把它们一次列全,免得用户只能手填猜名字。
pub(crate) fn cline_provider_candidates(config: &AppConfig) -> Result<Vec<(String, u32)>> {
    let binary = {
        let configured = config.plugins.cline.binary.trim();
        if configured.is_empty() {
            "cline"
        } else {
            configured
        }
    };
    let resolved = resolve_binary_path(binary)?;
    let cache_key = resolved.display().to_string();
    if let Ok(cache) = CLINE_PROVIDERS.lock() {
        if let Some((key, cached)) = cache.as_ref() {
            if key == &cache_key {
                return Ok(cached.clone());
            }
        }
    }
    let entry = locate_cline_llms(&resolved).with_context(|| {
        format!(
            "cline's bundled @cline/llms was not found above {}; the install may be incomplete",
            resolved.display()
        )
    })?;
    let script = format!(
        "const m = await import({});\n\
         const out = [];\n\
         for (const id of m.getProviderIds()) {{\n\
           let count = 0;\n\
           try {{ const models = await m.getModelsForProvider(id); count = Object.keys(models ?? {{}}).length; }} catch {{}}\n\
           out.push({{ id, count }});\n\
         }}\n\
         process.stdout.write(JSON.stringify(out));",
        serde_json::to_string(&entry.display().to_string())?,
    );
    let stdout = run_with_timeout(
        "node",
        &["--input-type=module", "-e", &script],
        CLI_LIST_TIMEOUT,
    )
    .context("running node for the cline provider list")?;
    let candidates = parse_cline_provider_candidates(&stdout)?;
    if let Ok(mut cache) = CLINE_PROVIDERS.lock() {
        *cache = Some((cache_key, candidates.clone()));
    }
    Ok(candidates)
}

/// 把裸名字/相对路径解析成实际文件路径(PATH 查找),供向上找包用。
fn resolve_binary_path(binary: &str) -> Result<PathBuf> {
    let path = PathBuf::from(binary);
    if path.is_absolute() || binary.contains(std::path::MAIN_SEPARATOR) {
        return Ok(path);
    }
    let search = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&search) {
        let candidate = dir.join(binary);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    bail!("{binary} not found in PATH")
}

/// 从 cline 可执行文件(已解符号链接)向上找 `@cline/llms` 入口。
/// 覆盖两种安装形态:npm 全局(`<prefix>/bin/cline` 符号链接到
/// `<prefix>/lib/node_modules/cline/bin/cline`)与 bin 目录里放真实脚本的形态。
fn locate_cline_llms(binary: &Path) -> Option<PathBuf> {
    let resolved = std::fs::canonicalize(binary).unwrap_or_else(|_| binary.to_path_buf());
    let mut dir = resolved.parent();
    while let Some(base) = dir {
        for suffix in [
            "node_modules/@cline/llms/dist/index.js",
            "lib/node_modules/cline/node_modules/@cline/llms/dist/index.js",
            "lib/node_modules/@cline/llms/dist/index.js",
        ] {
            let candidate = base.join(suffix);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        dir = base.parent();
    }
    None
}

/// `getModelsForProvider` 的条目:模型 id + 上下文窗口;输出前面可能混着
/// node 的杂音,从第一个 `[` 起解析。
fn parse_cline_models(stdout: &str) -> Result<Vec<LiveModel>> {
    let trimmed = stdout.trim();
    let start = trimmed
        .find('[')
        .context("the cline model catalog printed no JSON")?;
    let entries: Vec<serde_json::Value> =
        serde_json::from_str(&trimmed[start..]).context("the cline model catalog JSON")?;
    Ok(entries
        .into_iter()
        .filter_map(|entry| {
            let id = entry
                .get("id")
                .and_then(serde_json::Value::as_str)?
                .trim()
                .to_string();
            if id.is_empty() {
                return None;
            }
            let context_window = entry
                .get("contextWindow")
                .and_then(serde_json::Value::as_u64)
                .filter(|window| *window > 0);
            Some(LiveModel { id, context_window })
        })
        .collect())
}

/// `[{id,count}]`:供应商 id 与它的模型数。空 id 丢掉;不是数组就报错让调用方看见。
fn parse_cline_provider_candidates(stdout: &str) -> Result<Vec<(String, u32)>> {
    let trimmed = stdout.trim();
    let start = trimmed
        .find('[')
        .context("the cline provider list printed no JSON")?;
    let entries: Vec<serde_json::Value> =
        serde_json::from_str(&trimmed[start..]).context("the cline provider list JSON")?;
    let mut candidates = Vec::with_capacity(entries.len());
    for entry in entries {
        let Some(id) = entry.get("id").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let id = id.trim();
        if id.is_empty() {
            continue;
        }
        let count = entry
            .get("count")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0)
            .min(u32::MAX as u64) as u32;
        candidates.push((id.to_string(), count));
    }
    Ok(candidates)
}

/// `agy models`:每行 `slug<TAB>显示名`;首行 "Fetching available models..."
/// 之类的提示没有 TAB,自然被跳过。
pub(crate) fn parse_agy_models(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(slug, _)| slug.trim().to_string())
        .filter(|slug| !slug.is_empty())
        .collect()
}

/// `codex debug models`:`{"models":[{"slug":…,"visibility":"list"|"hide",…}]}`。
pub(crate) fn parse_codex_models(stdout: &str) -> Result<Vec<String>> {
    let start = stdout
        .find('{')
        .context("codex debug models printed no JSON")?;
    let value: serde_json::Value =
        serde_json::from_str(stdout[start..].trim()).context("codex debug models JSON")?;
    let models = value
        .get("models")
        .and_then(serde_json::Value::as_array)
        .context("codex debug models: missing models array")?;
    Ok(models
        .iter()
        .filter(|model| model.get("visibility").and_then(serde_json::Value::as_str) != Some("hide"))
        .filter_map(|model| model.get("slug").and_then(serde_json::Value::as_str))
        .map(str::to_string)
        .collect())
}

/// 跑子进程收 stdout,超时就杀。stdout 在独立线程里读:codex 的输出有几百
/// KB,超过管道缓冲,不边读边等会死锁。
fn run_with_timeout(binary: &str, args: &[&str], timeout: Duration) -> Result<String> {
    let mut child = Command::new(binary)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("failed to start {binary}"))?;
    let mut stdout = child.stdout.take().context("stdout pipe")?;
    let reader = std::thread::spawn(move || {
        let mut buffer = String::new();
        let _ = stdout.read_to_string(&mut buffer);
        buffer
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            bail!(
                "{binary} {} timed out after {}s",
                args.join(" "),
                timeout.as_secs()
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let output = reader.join().unwrap_or_default();
    if !status.success() {
        bail!("{binary} {} exited with {status}", args.join(" "));
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agy_listing_skips_the_banner_and_keeps_slugs() {
        let out = "Fetching available models...\ngemini-3.8-flash-high\tGemini 3.8 Flash (High)\nclaude-sonnet-4-6\tClaude Sonnet 4.6 (Thinking)\n\n";
        assert_eq!(
            parse_agy_models(out),
            vec![
                "gemini-3.8-flash-high".to_string(),
                "claude-sonnet-4-6".to_string()
            ]
        );
    }

    #[test]
    fn codex_listing_drops_hidden_models_and_tolerates_a_banner() {
        let out = "warming up\n{\"models\":[{\"slug\":\"gpt-reserve\",\"visibility\":\"hide\"},{\"slug\":\"gpt-5.6-luna\",\"visibility\":\"list\"},{\"slug\":\"gpt-5.5\"}]}";
        assert_eq!(
            parse_codex_models(out).unwrap(),
            vec!["gpt-5.6-luna".to_string(), "gpt-5.5".to_string()]
        );
        assert!(parse_codex_models("nothing here").is_err());
    }

    #[test]
    fn a_missing_cli_is_an_error_not_a_silent_preset() {
        let config = AppConfig::default();
        let mut provider = ProviderConfig::antigravity_template();
        provider.models = vec!["custom-alias".to_string()];
        let error =
            builtin_cli_catalog(&config, &provider, Some("/nonexistent/agy-binary")).unwrap_err();
        assert!(error.to_string().contains("failed to start"));
        // claude 没有列模型命令:目录就是别名表,并上手工加的。
        let mut claude = ProviderConfig::claude_code_template();
        claude.models = vec!["claude-fable-5".to_string()];
        assert!(builtin_cli_binary(&config, &claude).is_none());
        let models = builtin_cli_catalog(&config, &claude, None).unwrap();
        assert_eq!(models.len(), claude.preset_model_catalog().len() + 1);
        assert_eq!(models.last().map(String::as_str), Some("claude-fable-5"));
    }

    #[test]
    fn cline_catalog_is_custom_models_only() {
        let config = AppConfig::default();
        let mut provider = ProviderConfig::cline_template();
        provider.models = vec!["anthropic/claude-sonnet-4.6".to_string()];
        // cline 的目录走 `@cline/llms`(见 cline_catalog);预置表是空的,list
        // 不可用时目录就等于手工加的模型。
        let models = builtin_cli_catalog(&config, &provider, None).unwrap();
        assert_eq!(models, ["anthropic/claude-sonnet-4.6"]);
        assert_eq!(
            builtin_cli_binary(&config, &provider).as_deref(),
            Some("cline")
        );
    }

    #[test]
    fn cline_provider_candidates_read_the_printed_json() {
        // 空 id(纯空白)混在里面:丢掉,别拿它当候选。
        let out = "[{\"id\":\"cline\",\"count\":317},{\"id\":\"cline-pass\",\"count\":18},{\"id\":\" \",\"count\":3}]";
        assert_eq!(
            parse_cline_provider_candidates(out).unwrap(),
            vec![
                ("cline".to_string(), 317u32),
                ("cline-pass".to_string(), 18)
            ]
        );
        assert!(parse_cline_provider_candidates("no json here").is_err());
    }

    /// cline 的清单:node 的杂音在前、JSON 数组在后;空数组合法(调用方据此
    /// 报"没列出模型"),不是 JSON 就报错。
    #[test]
    fn cline_listing_parses_the_json_array() {
        let out = "some node noise\n[{\"id\":\"anthropic/claude-sonnet-4.6\",\"contextWindow\":1000000},{\"id\":\" cline-pass/kimi-k3 \"}]";
        let models = parse_cline_models(out).unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "anthropic/claude-sonnet-4.6");
        assert_eq!(models[0].context_window, Some(1_000_000));
        assert_eq!(models[1].id, "cline-pass/kimi-k3");
        assert_eq!(models[1].context_window, None);
        assert!(parse_cline_models("[]").unwrap().is_empty());
        assert!(parse_cline_models("nothing here").is_err());
    }

    /// 拉目录带回来的窗口进进程内缓存:激活时 `auto_configure_model_tags`
    /// 与 WebUI 的目录补全都从这里取,不再起一次 CLI。
    #[test]
    fn live_windows_are_remembered_for_later_activation() {
        let models = parse_cline_models(
            "[{\"id\":\"xiaomi/mimo-v2.6\",\"contextWindow\":262144},{\"id\":\"some/other\"}]",
        )
        .unwrap();
        remember_windows("cline", &models);
        assert_eq!(remembered_window("cline", "xiaomi/mimo-v2.6"), Some(262144));
        // 没给窗口的条目不记;另一家供应商的同名模型互不干扰。
        assert_eq!(remembered_window("cline", "some/other"), None);
        assert_eq!(remembered_window("codex", "xiaomi/mimo-v2.6"), None);
    }

    /// `@cline/llms` 的定位:从 cline 可执行文件(解掉符号链接后)向上找,
    /// npm 全局布局是 `<prefix>/lib/node_modules/cline/node_modules/@cline/llms`。
    #[test]
    fn cline_llms_is_located_by_walking_up_from_the_binary() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let dist = root.join("lib/node_modules/cline/node_modules/@cline/llms/dist");
        std::fs::create_dir_all(&dist).unwrap();
        std::fs::write(dist.join("index.js"), "// stub").unwrap();
        let bin_dir = root.join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let binary = bin_dir.join("cline");
        std::fs::write(&binary, "#!/usr/bin/env node\n").unwrap();
        // 被测函数先解掉二进制路径上的符号链接再往上走,期望值同样要解:
        // macOS 的临时目录 /var 是指向 /private/var 的符号链接。
        let expected = std::fs::canonicalize(dist.join("index.js")).unwrap();
        assert_eq!(locate_cline_llms(&binary), Some(expected));
    }
}

/// 真机探针:`cargo test --lib live_cli_catalog -- --ignored --nocapture`。
#[cfg(test)]
mod live_probe {
    use super::*;

    #[test]
    #[ignore]
    fn live_cli_catalog() {
        let config = AppConfig::default();
        for provider in [
            ProviderConfig::antigravity_template(),
            ProviderConfig::codex_template(),
            ProviderConfig::cline_template(),
        ] {
            let binary = builtin_cli_binary(&config, &provider);
            match builtin_cli_catalog(&config, &provider, binary.as_deref()) {
                Ok(models) => eprintln!(
                    "{}: {} models: {}",
                    provider.id,
                    models.len(),
                    models.join(", ")
                ),
                Err(error) => eprintln!("{}: ERROR {error:#}", provider.id),
            }
        }
    }
}
