//! 扩展的来源：互联网来的（git 克隆的 MCP 服务器、技能目录）还是自己创建的，
//! 以及互联网来的怎么检查更新、一键更新。
//!
//! pm 装的包另有锁文件记录来源与升级（`upgrade.rs`），这里管的是没经过 pm、
//! 直接 `git clone` 进来的东西。更新按它们自己的方式来：git 拉到远端默认分支
//! 的最新提交，再按目录里的文件同步依赖（npm / pnpm / yarn、uv / pip）。
//!
//! 克隆时常常直接检出某个提交（分离头指针、没有上游），所以不用 `git pull`：
//! 先查远端默认分支，抓它的最新提交，分支上就快进合并，分离的就切到新提交。
//! 已跟踪文件有本地修改时拒绝更新，不替用户决定丢掉哪一边。

use super::*;
use serde::Serialize;
use std::time::Duration;

const GIT_TIMEOUT: Duration = Duration::from_secs(90);
const DEPS_TIMEOUT: Duration = Duration::from_secs(600);
const LOG_TAIL_LINES: usize = 12;

#[derive(Clone, Debug, Serialize)]
pub struct Origin {
    /// git：互联网来的、能更新；self：自己创建或手动放进来的；managed：由别的程序管理（不在 gqy 目录里、也不是 git 仓库）
    pub kind: &'static str,
    /// git 仓库根目录；其余为空
    pub dir: Option<String>,
    pub remote: Option<String>,
    pub commit: Option<String>,
    /// 更新后要跑的依赖同步，按目录里的文件判断：npm / pnpm / yarn / uv / pip
    pub deps: Vec<&'static str>,
}

/// `path` 是扩展的文件或目录（技能目录、MCP 的脚本或可执行文件）。
pub fn detect_origin(path: &Path, gqy_root: &Path) -> Origin {
    let start = if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    if let Some(root) = git_root(start).filter(|root| !gqy_root.starts_with(root)) {
        let remote = git_sync(&root, &["remote", "get-url", "origin"]);
        let commit = git_sync(&root, &["rev-parse", "--short", "HEAD"]);
        return Origin {
            kind: "git",
            deps: dependency_steps(&root).iter().map(|step| step.0).collect(),
            dir: Some(root.display().to_string()),
            remote,
            commit,
        };
    }
    let kind = if path.starts_with(gqy_root) {
        "self"
    } else {
        "managed"
    };
    Origin {
        kind,
        dir: None,
        remote: None,
        commit: None,
        deps: Vec::new(),
    }
}

/// 仓库根目录。家目录是 dotfiles 仓库这类情况由调用方用 gqy 根目录挡掉。
fn git_root(start: &Path) -> Option<PathBuf> {
    git_sync(start, &["rev-parse", "--show-toplevel"]).map(PathBuf::from)
}

fn git_sync(dir: &Path, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(find_tool("git")?)
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (output.status.success() && !text.is_empty()).then_some(text)
}

/// daemon 的 PATH 常常不含 ~/.local/bin 之类的用户目录（uv、npm 装在那里），多找几处。
fn find_tool(name: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .chain([
            home.join(".local/bin"),
            home.join(".cargo/bin"),
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/usr/bin"),
        ])
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// (名字, 命令)。只在仓库根目录看，按常见组合挑一种，不叠加。
fn dependency_steps(root: &Path) -> Vec<(&'static str, Vec<String>)> {
    let has = |file: &str| root.join(file).exists();
    let venv_python = root.join(".venv/bin/python").display().to_string();
    let mut steps = Vec::new();
    if has("pnpm-lock.yaml") {
        steps.push(("pnpm", vec!["pnpm".into(), "install".into()]));
    } else if has("yarn.lock") {
        steps.push(("yarn", vec!["yarn".into(), "install".into()]));
    } else if has("package.json") {
        steps.push((
            "npm",
            vec![
                "npm".into(),
                "install".into(),
                "--no-audit".into(),
                "--no-fund".into(),
            ],
        ));
    }
    let pip = |extra: &[&str]| -> Vec<String> {
        let mut command = if find_tool("uv").is_some() {
            vec![
                "uv".into(),
                "pip".into(),
                "install".into(),
                "--python".into(),
                venv_python.clone(),
            ]
        } else {
            vec![
                venv_python.clone(),
                "-m".into(),
                "pip".into(),
                "install".into(),
            ]
        };
        command.extend(extra.iter().map(|part| part.to_string()));
        command
    };
    if has("uv.lock") {
        steps.push(("uv", vec!["uv".into(), "sync".into()]));
    } else if has("pyproject.toml") && has(".venv") {
        steps.push(("pip", pip(&["."])));
    } else if has("requirements.txt") && has(".venv") {
        steps.push(("pip", pip(&["-r", "requirements.txt"])));
    }
    steps
}

async fn run(dir: &Path, command: &[String], limit: Duration) -> Result<String> {
    let program = if command[0].contains('/') {
        PathBuf::from(&command[0])
    } else {
        find_tool(&command[0]).with_context(|| format!("{} is not installed", command[0]))?
    };
    let child = tokio::process::Command::new(program)
        .args(&command[1..])
        .current_dir(dir)
        .kill_on_drop(true)
        .output();
    let output = tokio::time::timeout(limit, child).await.with_context(|| {
        format!(
            "`{}` timed out after {}s",
            command.join(" "),
            limit.as_secs()
        )
    })??;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success() {
        bail!("`{}` failed:\n{}", command.join(" "), tail(&text));
    }
    Ok(text)
}

async fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let mut command = vec!["git".to_string()];
    command.extend(args.iter().map(|arg| arg.to_string()));
    Ok(run(dir, &command, GIT_TIMEOUT).await?.trim().to_string())
}

fn tail(text: &str) -> String {
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    lines[lines.len().saturating_sub(LOG_TAIL_LINES)..].join("\n")
}

#[derive(Debug, Serialize)]
pub struct UpdateCheck {
    pub current: String,
    pub latest: String,
    pub branch: String,
    pub has_update: bool,
}

/// 远端要跟的分支：本地分支有上游就用上游，否则用远端默认分支。
async fn target_branch(dir: &Path) -> Result<String> {
    if let Ok(upstream) = git(
        dir,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )
    .await
    {
        if let Some(branch) = upstream.strip_prefix("origin/") {
            return Ok(branch.to_string());
        }
    }
    let listing = git(dir, &["ls-remote", "--symref", "origin", "HEAD"]).await?;
    listing
        .lines()
        .find_map(|line| {
            line.strip_prefix("ref: refs/heads/")?
                .split_whitespace()
                .next()
        })
        .map(str::to_string)
        .context("could not find the remote default branch")
}

async fn fetch_latest(dir: &Path) -> Result<UpdateCheck> {
    let branch = target_branch(dir).await?;
    let shallow = git(dir, &["rev-parse", "--is-shallow-repository"]).await? == "true";
    let mut args = vec!["fetch", "--quiet", "origin", branch.as_str()];
    if shallow {
        args.insert(1, "--depth=1");
    }
    git(dir, &args).await?;
    let current = git(dir, &["rev-parse", "HEAD"]).await?;
    let latest = git(dir, &["rev-parse", "FETCH_HEAD"]).await?;
    Ok(UpdateCheck {
        has_update: current != latest,
        current,
        latest,
        branch,
    })
}

pub async fn check_update(dir: &Path) -> Result<UpdateCheck> {
    fetch_latest(dir).await
}

#[derive(Debug, Serialize)]
pub struct UpdateReport {
    pub updated: bool,
    pub from: String,
    pub to: String,
    pub log: Vec<String>,
}

pub async fn update(dir: &Path) -> Result<UpdateReport> {
    let dirty = git(dir, &["status", "--porcelain", "--untracked-files=no"]).await?;
    if !dirty.is_empty() {
        bail!(
            "this repository has local changes to tracked files; commit or discard them first:\n{}",
            tail(&dirty)
        );
    }
    let check = fetch_latest(dir).await?;
    let short = |sha: &str| sha.chars().take(7).collect::<String>();
    if !check.has_update {
        return Ok(UpdateReport {
            updated: false,
            from: short(&check.current),
            to: short(&check.latest),
            log: Vec::new(),
        });
    }
    let on_branch = git(dir, &["symbolic-ref", "-q", "HEAD"]).await.is_ok();
    let mut log = vec![format!(
        "git: {} -> {} ({})",
        short(&check.current),
        short(&check.latest),
        check.branch
    )];
    if on_branch {
        git(dir, &["merge", "--ff-only", "FETCH_HEAD"]).await?;
    } else {
        git(dir, &["checkout", "--quiet", "--detach", "FETCH_HEAD"]).await?;
    }
    for (name, command) in dependency_steps(dir) {
        let output = run(dir, &command, DEPS_TIMEOUT).await?;
        log.push(format!(
            "{name}: {}",
            tail(&output).lines().last().unwrap_or("done")
        ));
    }
    Ok(UpdateReport {
        updated: true,
        from: short(&check.current),
        to: short(&check.latest),
        log,
    })
}
