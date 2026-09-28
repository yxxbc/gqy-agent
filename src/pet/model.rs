//! 模型目录:读 `model3.json`,把散在 `motions/` 与 `E*/` 目录里的动作、表情
//! 补进清单。
//!
//! VTS 导出的模型常常把 `.motion3.json` / `.exp3.json` 直接摊在子目录里,而
//! `model3.json` 的 `Motions` / `Expressions` 字段是空的——VTS 自己扫目录,别的
//! 运行时(pixi-live2d-display 也是)按清单加载,于是动作与表情一个都用不上。
//! 这里**在内存里**把清单补全再交给页面,**不碰用户的文件**。

use anyhow::{bail, Context, Result};
use serde_json::json;
use std::path::{Path, PathBuf};

/// 一个可用的模型:目录 + 补全过的清单。
pub(in crate::pet) struct Model {
    /// 模型目录(绝对路径)。页面要的每个文件都从它下面按相对路径读。
    pub(in crate::pet) dir: PathBuf,
    /// 补全过 `Motions` / `Expressions` 的 model3.json 原文。
    pub(in crate::pet) manifest: String,
    /// model3.json 的文件名(页面用它拼 URL)。
    pub(in crate::pet) manifest_name: String,
    pub(in crate::pet) motions: usize,
    pub(in crate::pet) expressions: usize,
}

/// 从配置给的目录里找模型。目录里得有一份 `*.model3.json`,以及它指向的 `.moc3`。
pub(in crate::pet) fn load(dir: &Path) -> Result<Model> {
    if !dir.is_dir() {
        bail!("模型目录不存在: {}", dir.display());
    }
    let manifest_path = find_manifest(dir)?;
    let raw = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("读不了 {}", manifest_path.display()))?;
    let mut json: serde_json::Value = serde_json::from_str(&raw)
        .with_context(|| format!("{} 不是合法的 model3.json", manifest_path.display()))?;

    let moc = json
        .pointer("/FileReferences/Moc")
        .and_then(|value| value.as_str())
        .context("model3.json 的 FileReferences.Moc 缺失")?
        .to_string();
    if !dir.join(&moc).is_file() {
        bail!("model3.json 指向的 {} 不在模型目录里", moc);
    }

    let (motions, expressions) = register_assets(dir, &mut json)?;
    let manifest = serde_json::to_string(&json).context("重新序列化 model3.json 失败")?;
    let manifest_name = manifest_path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .context("model3.json 没有文件名")?;

    Ok(Model {
        dir: dir.to_path_buf(),
        manifest,
        manifest_name,
        motions,
        expressions,
    })
}

/// 目录里唯一的 `*.model3.json`。多于一份就报错——猜错了会加载到另一个模型。
fn find_manifest(dir: &Path) -> Result<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("读不了目录 {}", dir.display()))?
    {
        let path = entry?.path();
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if name.ends_with(".model3.json") {
            found.push(path);
        }
    }
    match found.len() {
        0 => bail!("{} 里没有 *.model3.json", dir.display()),
        1 => Ok(found.remove(0)),
        _ => bail!(
            "{} 里有 {} 份 model3.json,不知道该用哪一份",
            dir.display(),
            found.len()
        ),
    }
}

/// 把 `motions/` 与各 `*.exp3.json` 注册进清单。返回(动作数, 表情数)。
///
/// 动作按文件名分组:`idle.motion3.json` → 组名 `Idle`(运行时找这个组做待机循环,
/// 名字大小写要对),别的用原文件名。表情用文件名作品签名。
fn register_assets(dir: &Path, json: &mut serde_json::Value) -> Result<(usize, usize)> {
    let mut motions: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
    let mut expressions: Vec<serde_json::Value> = Vec::new();
    let mut walked: Vec<PathBuf> = Vec::new();
    collect(dir, &mut walked)?;
    walked.sort();
    for path in &walked {
        let Some(relative) = path
            .strip_prefix(dir)
            .ok()
            .map(|rel| rel.to_string_lossy().replace('\\', "/"))
        else {
            continue;
        };
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if name.ends_with(".motion3.json") {
            let stem = name.trim_end_matches(".motion3.json").to_string();
            let group = if stem.eq_ignore_ascii_case("idle") {
                "Idle".to_string()
            } else {
                stem
            };
            let entry = json!({ "File": relative });
            motions
                .entry(group)
                .or_insert_with(|| serde_json::Value::Array(Vec::new()))
                .as_array_mut()
                .expect("刚建的是数组")
                .push(entry);
        } else if name.ends_with(".exp3.json") {
            let stem = name.trim_end_matches(".exp3.json").to_string();
            expressions.push(json!({ "Name": stem, "File": relative }));
        }
    }

    let references = json
        .get_mut("FileReferences")
        .and_then(|value| value.as_object_mut())
        .context("model3.json 里没有 FileReferences")?;
    // 清单里已经有东西就留着:那是模型作者写的,比我们扫出来的准。
    if !motions.is_empty() {
        let existing = references
            .get("Motions")
            .and_then(|value| value.as_object())
            .map(|object| object.len())
            .unwrap_or(0);
        if existing == 0 {
            references.insert(
                "Motions".to_string(),
                serde_json::Value::Object(motions.clone()),
            );
        } else {
            motions.clear();
        }
    }
    if !expressions.is_empty() {
        let existing = references
            .get("Expressions")
            .and_then(|value| value.as_array())
            .map(|array| array.len())
            .unwrap_or(0);
        if existing == 0 {
            references.insert(
                "Expressions".to_string(),
                serde_json::Value::Array(expressions.clone()),
            );
        } else {
            expressions.clear();
        }
    }
    Ok((motions.len(), expressions.len()))
}

/// 递归收集文件(模型的子目录不深,但 VTS 导出会把动作放在 `motions/`、表情放在
/// `EXP3/`,所以得往下一层看)。
fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("读不了 {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            collect(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}
