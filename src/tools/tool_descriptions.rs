use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadPolicy {
    Summary,
    Group,
    Hidden,
}

impl Default for LoadPolicy {
    fn default() -> Self {
        Self::Summary
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolDescription {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub parameters: Value,
    pub always_loaded: bool,
    #[serde(default)]
    pub load_policy: LoadPolicy,
    #[serde(default)]
    pub groups: Vec<String>,
    /// 按工具超时（秒）。缺省=吃 registry 默认兜底；0=豁免（自管超时或
    /// 天生长跑的工具，如 run_command/subagent）。
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
    /// 场所信任位:`"trust": "external"` 的工具也给不可信入口(QQ 群、远端
    /// WebUI 成员)。缺省只给属主。受限注册表就是按这个位从全量面上筛出来的。
    #[serde(default)]
    pub trust: crate::tools::ToolTrust,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolGroupDescription {
    pub summary: String,
}

static TOOL_DESCRIPTIONS: OnceLock<HashMap<String, ToolDescription>> = OnceLock::new();
static TOOL_GROUPS: OnceLock<HashMap<String, ToolGroupDescription>> = OnceLock::new();
const TOOL_GROUPS_RAW: &str = include_str!("descriptions/groups.json");

// `TOOL_DESCRIPTION_FILES`:build.rs 扫 `descriptions/` 目录生成,每个
// `*.json`(groups.json 除外)一条 include_str!。以前是手写清单,新增 JSON
// 忘了补一行就静默退回 Rust 占位描述;现在丢进目录即生效。
include!(concat!(env!("OUT_DIR"), "/tool_description_files.rs"));

pub fn all() -> &'static HashMap<String, ToolDescription> {
    TOOL_DESCRIPTIONS.get_or_init(|| {
        let mut map = HashMap::new();
        for (file, raw) in TOOL_DESCRIPTION_FILES {
            let desc: ToolDescription = serde_json::from_str(raw).unwrap_or_else(|error| {
                panic!("built-in tool description {file} must be valid JSON: {error}")
            });
            let name = desc.name.clone();
            assert!(
                map.insert(name.clone(), desc).is_none(),
                "tool description {file} reuses the name `{name}`"
            );
        }
        map
    })
}

pub fn get(name: &str) -> Option<&'static ToolDescription> {
    all().get(name)
}

pub fn group_summary(group: &str) -> String {
    groups()
        .get(group)
        .map(|desc| desc.summary.clone())
        .unwrap_or_else(|| group.to_string())
}

fn groups() -> &'static HashMap<String, ToolGroupDescription> {
    TOOL_GROUPS.get_or_init(|| {
        serde_json::from_str(TOOL_GROUPS_RAW).expect("tool group description JSON must be valid")
    })
}

#[cfg(test)]
pub fn group_names() -> Vec<&'static str> {
    groups().keys().map(String::as_str).collect()
}
