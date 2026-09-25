//! 管理面（WebUI 设置 → 插件 →「扩展」、终端配置器）用的技能清单与操作。
//!
//! 模型面的目录走 `discover`，关掉的技能在那里看不见；管理面要把关掉的也列出来，
//! 才能再打开。开关按来源分三种（与 `allowed_by` 的门一一对应）：
//!
//! - `fixed`：平台级内置技能（skill-creator 等），任何人格都开着，不给关。
//! - `marker`：人格自己那一层的技能不受白名单管（她用 manage_skill 建的就在这层，
//!   受白名单管的话新建的会默认看不见），开关是目录里的 `.disabled` 标记，与
//!   `gqy skills disable` 同一个机制。
//! - `whitelist`：全局层与可选内置技能，开关写进当前人格清单的 `plugins.skills`
//!   白名单，与成员引导、自选功能同一套。白名单原本是 None（= 全部）时，第一次
//!   关某一个会把它展开成「当前开着的全部」再去掉这一个。

use super::*;
use crate::config::PersonaManifest;

const DISABLED_MARKER: &str = ".disabled";

#[derive(Clone, Debug, Serialize)]
pub(crate) struct AdminSkill {
    pub(crate) name: String,
    pub(crate) description: String,
    /// persona / global / built_in
    pub(crate) source: &'static str,
    pub(crate) enabled: bool,
    /// fixed / marker / whitelist
    pub(crate) toggle: &'static str,
    /// 技能目录；内置技能为空。
    pub(crate) path: Option<String>,
}

/// 全部技能（含关掉的），同名时人格层优先于全局层、目录优先于内置，与模型面一致。
pub(crate) fn admin_catalog(config: &AppConfig, paths: &GqyPaths) -> Result<Vec<AdminSkill>> {
    let allowlist = skill_allowlist(config, paths);
    let default_persona = is_default_persona(config);
    let mut skills = Vec::new();
    let mut seen = BTreeSet::new();
    for (root, source) in skill_roots(config, paths) {
        for directory in sorted_skill_directories(&root)? {
            let skill_file = directory.join("SKILL.md");
            if !skill_file.is_file() {
                continue;
            }
            let directory_name = directory
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            let Ok(metadata) = read_skill_file(&skill_file)
                .and_then(|raw| parse_skill_metadata(&raw, Some(directory_name)))
            else {
                continue;
            };
            if !seen.insert(metadata.name.clone()) {
                continue;
            }
            let marked_off = directory.join(DISABLED_MARKER).exists();
            let (enabled, toggle) = if source == SkillSource::Persona {
                (!marked_off, "marker")
            } else {
                let allowed = allowed_by(default_persona, &allowlist, &metadata.name, source);
                (allowed && !marked_off, "whitelist")
            };
            skills.push(AdminSkill {
                name: metadata.name,
                description: metadata.description,
                source: source.as_str(),
                enabled,
                toggle,
                path: Some(directory.display().to_string()),
            });
        }
    }
    for (name, raw, platform_wide) in BUILTIN_SKILLS {
        if seen.contains(*name) {
            continue;
        }
        let metadata = parse_skill_metadata(raw, Some(name))?;
        let (enabled, toggle) = if *platform_wide {
            (true, "fixed")
        } else {
            let allowed = allowed_by(default_persona, &allowlist, name, SkillSource::BuiltIn);
            (allowed, "whitelist")
        };
        skills.push(AdminSkill {
            name: metadata.name,
            description: metadata.description,
            source: SkillSource::BuiltIn.as_str(),
            enabled,
            toggle,
            path: None,
        });
    }
    Ok(skills)
}

fn find(config: &AppConfig, paths: &GqyPaths, name: &str) -> Result<AdminSkill> {
    admin_catalog(config, paths)?
        .into_iter()
        .find(|skill| skill.name == name)
        .with_context(|| format!("skill not found: {name}"))
}

pub(crate) fn set_skill_enabled(
    config: &AppConfig,
    paths: &GqyPaths,
    name: &str,
    enabled: bool,
) -> Result<()> {
    let skill = find(config, paths, name)?;
    match skill.toggle {
        "fixed" => bail!("{name} is a platform built-in skill and is always on"),
        "marker" => set_marker(skill.path.as_deref(), enabled),
        _ => {
            if enabled {
                // 全局层被 `gqy skills disable` 打过标记的，打开时一并去掉
                set_marker(skill.path.as_deref(), true)?;
            }
            let catalog = admin_catalog(config, paths)?;
            let scope = config.active_persona_scope();
            let mut manifest = PersonaManifest::load(config, paths, &scope);
            let mut list = manifest.plugins.skills.clone().unwrap_or_else(|| {
                catalog
                    .iter()
                    .filter(|entry| entry.toggle == "whitelist" && entry.enabled)
                    .map(|entry| entry.name.clone())
                    .collect()
            });
            list.retain(|entry| entry != name);
            if enabled {
                list.push(name.to_string());
            }
            list.sort();
            manifest.plugins.skills = Some(list);
            let path = PersonaManifest::manifest_path(config, paths, &scope);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
            fs::write(&path, manifest.to_toml())
                .with_context(|| format!("failed to write {}", path.display()))
        }
    }
}

fn set_marker(directory: Option<&str>, enabled: bool) -> Result<()> {
    let Some(directory) = directory else {
        return Ok(());
    };
    let marker = Path::new(directory).join(DISABLED_MARKER);
    if enabled {
        match fs::remove_file(&marker) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
            _ => Ok(()),
        }
    } else {
        fs::write(&marker, "disabled\n")
            .with_context(|| format!("failed to write {}", marker.display()))
    }
}

/// 新建技能：与她用 manage_skill 建的一样落在当前人格那一层，走同一套草稿发布
/// （名字与描述校验、打包上限、原子交换都在里面）。
pub(crate) fn create_skill(
    config: &AppConfig,
    paths: &GqyPaths,
    name: &str,
    description: &str,
    body: &str,
) -> Result<PublishedSkill> {
    let draft = create_draft(config, paths, name, description, SkillScope::Persona)?;
    let body = body.trim();
    let body = if body.is_empty() {
        format!("# {name}\n\nDescribe the reusable workflow here.")
    } else {
        body.to_string()
    };
    fs::write(
        &draft.skill_file,
        format!(
            "---\nname: {name}\ndescription: {}\n---\n\n{body}\n",
            serde_json::to_string(description.trim())?
        ),
    )
    .with_context(|| format!("failed to write {}", draft.skill_file))?;
    publish_draft(paths, &draft.id)
}

/// 删除目录里的技能；内置技能没有文件，不能删。
pub(crate) fn remove_skill(config: &AppConfig, paths: &GqyPaths, name: &str) -> Result<()> {
    let skill = find(config, paths, name)?;
    let scope = match skill.source {
        "persona" => SkillScope::Persona,
        "global" => SkillScope::Global,
        _ => bail!("{name} is built in and cannot be deleted"),
    };
    delete_skill(config, paths, name, scope).map(|_| ())
}

/// SKILL.md 全文，给抽屉里查看用。
pub(crate) fn skill_source_text(
    config: &AppConfig,
    paths: &GqyPaths,
    name: &str,
) -> Result<String> {
    let skill = find(config, paths, name)?;
    match skill.path {
        Some(directory) => read_skill_file(&Path::new(&directory).join("SKILL.md")),
        None => BUILTIN_SKILLS
            .iter()
            .find(|(builtin, _, _)| *builtin == name)
            .map(|(_, raw, _)| raw.to_string())
            .context("built-in skill source missing"),
    }
}
