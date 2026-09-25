//! 升级一个已装的包，拆成「准备」与「执行」两步：命令行在两步之间问一句确认，
//! WebUI 的升级按钮本身就是确认，直接连着调。

use super::*;
use crate::config::AppConfig;

/// 已抓取、已比对、等待执行的升级。`fetched` 持有临时目录，执行前不能丢。
pub struct PreparedUpgrade {
    pub name: String,
    pub from_version: String,
    pub to_version: String,
    source: PackageSource,
    plan: InstallPlan,
    fetched: FetchedPackage,
}

/// 锁文件里记的来源可能是 `owner/repo`、URL、本地目录，也可能只是 tap 里的包名。
pub async fn resolve_source(
    paths: &GqyPaths,
    spec: &str,
    reference: Option<&str>,
) -> Result<PackageSource> {
    match PackageSource::parse_spec(spec, reference)? {
        Some(source) => Ok(source),
        None => resolve_from_taps(paths, spec).await,
    }
}

/// 重新抓来源并比对。已是最新返回 None。
pub async fn prepare_upgrade(
    config: &AppConfig,
    paths: &GqyPaths,
    name: &str,
) -> Result<Option<PreparedUpgrade>> {
    let lock = load_lock(paths)?;
    let installed = lock
        .packages
        .get(name)
        .with_context(|| format!("package {name:?} is not installed"))?;
    let source = resolve_source(paths, &installed.source, installed.reference.as_deref()).await?;
    let fetched = fetch(&source).await?;
    let plan = plan_install(config, paths, &fetched.root)?;
    if plan.manifest.package.name != name {
        bail!(
            "{} now serves package {:?}, not {name:?}",
            source.describe(),
            plan.manifest.package.name
        );
    }
    if is_up_to_date(installed, &plan, fetched.commit.as_deref())? {
        return Ok(None);
    }
    Ok(Some(PreparedUpgrade {
        name: name.to_string(),
        from_version: installed.version.clone(),
        to_version: plan.manifest.package.version.clone(),
        source,
        plan,
        fetched,
    }))
}

pub fn apply_upgrade(paths: &GqyPaths, prepared: PreparedUpgrade) -> Result<InstalledPackage> {
    let commit = prepared.fetched.commit.clone();
    install(paths, &prepared.plan, &prepared.source, commit, false)
}
