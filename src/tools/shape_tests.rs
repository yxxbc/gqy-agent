//! 三张注册表的形状指纹(09-10 分层架构阶段 4 的安全网):名字 → 定义 JSON 的
//! sha256。三表合一之后 normal/dev/受限三个面的 tools 数组必须逐字节不变——
//! 这是 AGENTS §1.1 的前缀契约,也是 QQ 会话不冷启动的保证。刻意的变化要改
//! 夹具并在提交说明里写清楚。

use super::*;
use sha2::Digest;
use std::collections::BTreeMap;

const FIXTURE: &str = include_str!("fixtures/registry-shapes.json");

fn shape(registry: &ToolRegistry) -> BTreeMap<String, String> {
    registry
        .definitions()
        .into_iter()
        .map(|definition| {
            let payload = serde_json::to_string(&definition).unwrap();
            let digest = sha2::Sha256::digest(payload.as_bytes());
            (definition.function.name.clone(), hex::encode(digest))
        })
        .collect()
}

fn current() -> serde_json::Value {
    let temp = tempfile::tempdir().unwrap();
    let paths = tests::test_paths(temp.path());
    let config = AppConfig::default();
    serde_json::json!({
        "normal": shape(&builtin_registry(&config, &paths)),
        "dev": shape(&dev_registry(&config, &paths)),
        "restricted": shape(&restricted_platform_registry(&config, &paths)),
    })
}

#[test]
fn registry_shapes_match_fixture() {
    let expected: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();
    let actual = current();
    for face in ["normal", "dev", "restricted"] {
        let want = expected[face].as_object().cloned().unwrap_or_default();
        let got = actual[face].as_object().cloned().unwrap_or_default();
        let missing: Vec<_> = want.keys().filter(|k| !got.contains_key(*k)).collect();
        let extra: Vec<_> = got.keys().filter(|k| !want.contains_key(*k)).collect();
        let changed: Vec<_> = want
            .iter()
            .filter(|(k, v)| got.get(*k).is_some_and(|g| g != *v))
            .map(|(k, _)| k)
            .collect();
        assert!(
            missing.is_empty() && extra.is_empty() && changed.is_empty(),
            "{face} face drifted: missing={missing:?} extra={extra:?} changed={changed:?} \
             (run `cargo test write_registry_shape_fixture -- --ignored` if intended)"
        );
    }
}

/// 只有 Rust 侧描述、刻意不走 descriptions/*.json 的内置工具。其余工具注册时
/// 若找不到 JSON,`apply_built_in_description` 会静默保留 Rust 占位描述——而
/// 刷新夹具会把这种错误状态一并钉进基线,所以单独拦一道。
const RUST_ONLY_DESCRIPTIONS: &[&str] = &[
    "job",
    "load_tools",
    "query_token_usage",
    "send_subagent_message",
];

#[test]
fn registered_built_in_tools_have_json_descriptions() {
    let temp = tempfile::tempdir().unwrap();
    let paths = tests::test_paths(temp.path());
    let config = AppConfig::default();
    let mut missing = std::collections::BTreeSet::new();
    for registry in [
        builtin_registry(&config, &paths),
        dev_registry(&config, &paths),
        restricted_platform_registry(&config, &paths),
    ] {
        for name in registry.tool_names() {
            if tool_descriptions::get(&name).is_none()
                && !RUST_ONLY_DESCRIPTIONS.contains(&name.as_str())
            {
                missing.insert(name);
            }
        }
    }
    assert!(
        missing.is_empty(),
        "tools registered without src/tools/descriptions/*.json: {missing:?}"
    );
}

#[test]
#[ignore]
fn write_registry_shape_fixture() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/tools/fixtures/registry-shapes.json");
    std::fs::write(&path, serde_json::to_string_pretty(&current()).unwrap()).unwrap();
    println!("wrote {}", path.display());
}
