//! `gqy github`:顾清影 自己的 GitHub bot 账号。登录态只写 `<GQY_HOME>/github/`,
//! 宿主的 gh / git 登录不受影响。`github` 工具只在 `as_bot=true` 时用它。

use crate::cli::*;
use crate::tools::github::{BotAccount, BotHome};
use std::process::Command as Process;

#[derive(Debug, Args)]
pub struct GithubArgs {
    #[command(subcommand)]
    pub command: GithubCommand,
}

#[derive(Debug, Subcommand)]
pub enum GithubCommand {
    /// 登录 bot 账号(默认走浏览器,`--with-token` 从标准输入读 token)
    Login {
        #[arg(long)]
        with_token: bool,
    },
    /// 查看 bot 身份与你自己的 gh 身份
    Status,
    /// 退出 bot 账号并删掉本地记录
    Logout,
}

pub async fn run_github(paths: &GqyPaths, args: GithubArgs) -> Result<()> {
    let home = BotHome::new(paths);
    match args.command {
        GithubCommand::Login { with_token } => github_login(&home, with_token),
        GithubCommand::Status => github_status(&home),
        GithubCommand::Logout => github_logout(&home),
    }
}

fn bot_gh(home: &BotHome) -> Process {
    let mut command = Process::new("gh");
    home.apply_gh_dir(&mut command);
    command
}

fn github_login(home: &BotHome, with_token: bool) -> Result<()> {
    home.ensure_dirs()?;
    let mut command = bot_gh(home);
    // --insecure-storage:token 写进 bot 自己的 hosts.yml(0600),不进 Keychain,
    // 免得和宿主账号的钥匙串条目搅在一起。
    command.args([
        "auth",
        "login",
        "--hostname",
        "github.com",
        "--git-protocol",
        "https",
        "--insecure-storage",
    ]);
    command.arg(if with_token { "--with-token" } else { "--web" });
    let status = command.status().context(t(
        "failed to run gh. Install GitHub CLI first",
        "无法运行 gh，请先安装 GitHub CLI",
    ))?;
    if !status.success() {
        bail!("{}", t("gh auth login failed", "gh 登录失败"));
    }
    let account = fetch_bot_account(home)?;
    if host_login().as_deref() == Some(account.login.as_str()) {
        eprintln!(
            "{}",
            t(
                "warning: the bot account is your own gh account, so commits cannot tell you and GQY apart",
                "警告：bot 账号就是你自己的 gh 账号，提交记录将无法区分你和顾清影",
            )
        );
    }
    home.save_account(&account)?;
    println!(
        "{} {} <{}>",
        t("Bot identity ready:", "bot 身份已就绪："),
        account.login,
        account.noreply_email()
    );
    println!(
        "{} {}",
        t("Credentials:", "凭据目录："),
        home.root().display()
    );
    Ok(())
}

fn fetch_bot_account(home: &BotHome) -> Result<BotAccount> {
    // 只认 bot 目录里存下的 token:让 gh 自己找的话,它找不到会回退到 Keychain
    // 里宿主的 token,记下来的就成了宿主账号。
    let token = home.token().context(t(
        "gh did not store the token in the bot directory",
        "gh 没有把 token 存进 bot 目录",
    ))?;
    let output = bot_gh(home)
        .env("GH_TOKEN", token)
        .args(["api", "user"])
        .output()
        .context("failed to run gh api user")?;
    if !output.status.success() {
        bail!(
            "gh api user failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let user: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    Ok(BotAccount {
        login: user["login"]
            .as_str()
            .context("gh api user returned no login")?
            .to_string(),
        id: user["id"].as_u64().context("gh api user returned no id")?,
    })
}

/// 宿主自己的 gh 登录,不带任何 bot 环境。
fn host_login() -> Option<String> {
    let output = Process::new("gh")
        .args(["api", "user", "--jq", ".login"])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|login| !login.is_empty())
}

fn github_status(home: &BotHome) -> Result<()> {
    match home.account() {
        Some(account) => {
            println!("bot:  {} <{}>", account.login, account.noreply_email());
            let _ = bot_gh(home)
                .args(["auth", "status", "--hostname", "github.com"])
                .status();
        }
        None => println!(
            "{}",
            t(
                "bot:  not logged in (run `gqy github login`)",
                "bot：未登录（执行 `gqy github login`）",
            )
        ),
    }
    match host_login() {
        Some(login) => println!("host: {login}"),
        None => println!("{}", t("host: not logged in", "host：未登录")),
    }
    println!(
        "{} {}",
        t("Credentials:", "凭据目录："),
        home.root().display()
    );
    Ok(())
}

fn github_logout(home: &BotHome) -> Result<()> {
    if let Some(account) = home.account() {
        let _ = bot_gh(home)
            .args([
                "auth",
                "logout",
                "--hostname",
                "github.com",
                "--user",
                &account.login,
            ])
            .status();
    }
    home.clear_account()?;
    println!("{}", t("Bot identity removed.", "bot 身份已移除。"));
    Ok(())
}
