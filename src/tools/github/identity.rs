//! 顾清影 在 GitHub 上用谁的身份。
//!
//! 缺省是宿主身份:子进程环境原样继承,用的就是用户自己的 gh / git 登录态。只有
//! 用户明确要求时才切到 bot——bot 的全部状态住在 `<GQY_HOME>/github/`(独立的
//! `GH_CONFIG_DIR` 与 gitconfig),宿主的 `~/.config/gh`、`~/.gitconfig`、
//! Keychain 一个字节都不碰。

use crate::paths::GqyPaths;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

/// 宿主环境里能压过 `GH_CONFIG_DIR` 登录态的 token 变量。bot 模式必须剥掉:
/// gh 里环境变量 token 的优先级高于 hosts.yml,不剥就静默用了宿主 token。
const HOST_TOKEN_ENV: &[&str] = &[
    "GH_TOKEN",
    "GITHUB_TOKEN",
    "GH_ENTERPRISE_TOKEN",
    "GITHUB_ENTERPRISE_TOKEN",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Identity {
    Host,
    Bot,
}

impl Identity {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Bot => "bot",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BotAccount {
    pub login: String,
    pub id: u64,
}

impl BotAccount {
    /// GitHub 按邮箱关联账号:只有 `<id>+<login>@users.noreply.github.com` 这种
    /// 形状才会在 commit 上挂出 bot 的头像并计入它的贡献。
    pub(crate) fn noreply_email(&self) -> String {
        format!("{}+{}@users.noreply.github.com", self.id, self.login)
    }
}

/// bot 身份跑命令所需的全部东西。不派生 Debug:token 不能进日志。
#[derive(Clone)]
pub(crate) struct BotCredentials {
    pub account: BotAccount,
    token: String,
}

#[derive(Debug, Clone)]
pub(crate) struct BotHome {
    root: PathBuf,
}

impl BotHome {
    pub(crate) fn new(paths: &GqyPaths) -> Self {
        Self {
            root: paths.root_dir.join("github"),
        }
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn gh_config_dir(&self) -> PathBuf {
        self.root.join("gh")
    }

    pub(crate) fn gitconfig_path(&self) -> PathBuf {
        self.root.join("gitconfig")
    }

    fn account_path(&self) -> PathBuf {
        self.root.join("account.json")
    }

    fn hosts_path(&self) -> PathBuf {
        self.gh_config_dir().join("hosts.yml")
    }

    /// `gh auth login --insecure-storage` 写进 bot 目录的 token。
    pub(crate) fn token(&self) -> Option<String> {
        let raw = std::fs::read_to_string(self.hosts_path()).ok()?;
        hosts_token(&raw)
    }

    pub(crate) fn credentials(&self) -> Result<BotCredentials> {
        let account = self.account().context(
            "bot identity is not configured. Ask the user to run `gqy github login` first",
        )?;
        let token = self.token().context(
            "the bot login has no stored token. Ask the user to run `gqy github login` again",
        )?;
        Ok(BotCredentials { account, token })
    }

    pub(crate) fn account(&self) -> Option<BotAccount> {
        let raw = std::fs::read_to_string(self.account_path()).ok()?;
        serde_json::from_str(&raw).ok()
    }

    pub(crate) fn ensure_dirs(&self) -> Result<()> {
        crate::paths::ensure_private_dir(&self.root)
            .with_context(|| format!("failed to create {}", self.root.display()))?;
        let gh_dir = self.gh_config_dir();
        crate::paths::ensure_private_dir(&gh_dir)
            .with_context(|| format!("failed to create {}", gh_dir.display()))?;
        Ok(())
    }

    pub(crate) fn save_account(&self, account: &BotAccount) -> Result<()> {
        self.ensure_dirs()?;
        let gitconfig = self.gitconfig_path();
        std::fs::write(&gitconfig, gitconfig_text(account))
            .with_context(|| format!("failed to write {}", gitconfig.display()))?;
        let account_path = self.account_path();
        std::fs::write(&account_path, serde_json::to_string_pretty(account)?)
            .with_context(|| format!("failed to write {}", account_path.display()))?;
        Ok(())
    }

    pub(crate) fn clear_account(&self) -> Result<()> {
        for path in [self.account_path(), self.gitconfig_path()] {
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("failed to remove {}", path.display()))
                }
            }
        }
        Ok(())
    }

    /// 成员/`/sandbox` 会话的 Landlock 不放行凭据目录时,gh 会报一句看不懂的
    /// 权限错误。提前说清楚该往哪加。
    pub(crate) fn check_sandbox_access(&self) -> Result<()> {
        if let Some(policy) = crate::tools::sandbox::current_sandbox() {
            if !policy
                .read_write
                .iter()
                .any(|allowed| self.root.starts_with(allowed))
            {
                bail!(
                    "bot identity is blocked by this session's sandbox. Its credentials live in {}. The user must add that directory to tools.sandbox.writable",
                    self.root.display()
                );
            }
        }
        Ok(())
    }

    /// 只指 gh 登录目录。登录、登出时还没有账号记录,用这个。
    pub(crate) fn apply_gh_dir(&self, command: &mut Command) {
        command.env("GH_CONFIG_DIR", self.gh_config_dir());
        for key in HOST_TOKEN_ENV {
            command.env_remove(key);
        }
    }

    pub(crate) fn apply_bot(&self, command: &mut Command, credentials: &BotCredentials) {
        let account = &credentials.account;
        self.apply_gh_dir(command);
        command
            // 显式给 token,不让 gh 自己找:bot 目录里没有登录时,gh 会回退去读
            // Keychain 里宿主那条 `gh:github.com`,把宿主 token 递给 git(09-15
            // 实测,哈希与宿主 token 一致)。环境变量优先级最高,回退走不到。
            .env("GH_TOKEN", &credentials.token)
            .env("GIT_CONFIG_GLOBAL", self.gitconfig_path())
            // 系统级 gitconfig(Xcode / Homebrew 带的)常挂着 osxkeychain 凭据
            // 助手,会把宿主的 GitHub 凭据递给 bot 的 push。
            .env("GIT_CONFIG_NOSYSTEM", "1")
            // 仓库本地的 user.* 会盖过全局配置,committer 用环境变量钉死成 bot。
            .env("GIT_COMMITTER_NAME", &account.login)
            .env("GIT_COMMITTER_EMAIL", account.noreply_email())
            // SSH 远端会拿宿主的私钥推送。bot 模式一律走 HTTPS + gh 凭据助手。
            .env("GIT_SSH_COMMAND", "false");
    }
}

/// 从 gh 的 hosts.yml 取 github.com 的 token:旧布局在主机层,多账号布局在
/// `users.<当前用户>` 下。
pub(crate) fn hosts_token(raw: &str) -> Option<String> {
    let docs = yaml_rust2::YamlLoader::load_from_str(raw).ok()?;
    let host = &docs.first()?["github.com"];
    let direct = host["oauth_token"].as_str();
    let via_user = host["user"]
        .as_str()
        .and_then(|user| host["users"][user]["oauth_token"].as_str());
    direct
        .or(via_user)
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_string)
}

/// 两种身份都要:子进程没有终端,任何交互提示都只会卡到超时。
pub(crate) fn apply_non_interactive(command: &mut Command) {
    command
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("GIT_TERMINAL_PROMPT", "0");
}

/// bot 专用 gitconfig。空的 `helper =` 清掉之前继承来的凭据助手,再只挂 gh 的
/// ——它读的是同一个 `GH_CONFIG_DIR`,push 用的就是 bot token。
pub(crate) fn gitconfig_text(account: &BotAccount) -> String {
    format!(
        "# Managed by gqy (`gqy github login`). Rewritten on every login.\n\
         [user]\n\
         \tname = {login}\n\
         \temail = {email}\n\
         [credential]\n\
         \thelper =\n\
         [credential \"https://github.com\"]\n\
         \thelper =\n\
         \thelper = !gh auth git-credential\n\
         [credential \"https://gist.github.com\"]\n\
         \thelper =\n\
         \thelper = !gh auth git-credential\n",
        login = account.login,
        email = account.noreply_email(),
    )
}
