//! `github` 工具的各个 action。每个 action 是几条 git / gh 子进程串起来,身份在
//! [`Runner`] 里一次定好,后面每条命令吃同一份环境。

use super::attribution::append_trailer;
use super::identity::{apply_non_interactive, BotCredentials, Identity};
use super::GithubContext;
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

/// 单条子进程的上限。push 大仓库、fork 冷启动都可能要几十秒。工具在
/// descriptions 里豁免了兜底超时,真正的闸在这里。
const STEP_TIMEOUT: Duration = Duration::from_secs(120);

const MAX_OUTPUT_CHARS: usize = 20_000;

pub(super) async fn run(context: &GithubContext, args: Value) -> Result<String> {
    let action = args
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("status");
    let identity = if bool_arg(&args, "as_bot") {
        Identity::Bot
    } else {
        Identity::Host
    };
    let runner = Runner::new(context, identity)?;
    match action {
        "status" => status(context, &runner).await,
        "commit" => commit(context, &runner, &args).await,
        "pr_create" => pr_create(context, &runner, &args).await,
        "issue_create" => issue_create(context, &runner, &args).await,
        "comment" => comment(&runner, &args).await,
        "gh" => passthrough(&runner, &args).await,
        other => bail!(
            "unknown action: {other}. Use status, commit, pr_create, issue_create, comment, or gh."
        ),
    }
}

struct Runner<'a> {
    context: &'a GithubContext,
    identity: Identity,
    bot: Option<BotCredentials>,
    workdir: PathBuf,
}

impl<'a> Runner<'a> {
    fn new(context: &'a GithubContext, identity: Identity) -> Result<Self> {
        let bot = match identity {
            Identity::Host => None,
            Identity::Bot => {
                context.home.check_sandbox_access()?;
                Some(context.home.credentials()?)
            }
        };
        Ok(Self {
            context,
            identity,
            bot,
            workdir: crate::tools::workspace::effective_workdir(),
        })
    }

    async fn output(
        &self,
        bot: Option<&BotCredentials>,
        program: &str,
        args: &[&str],
    ) -> Result<std::process::Output> {
        let mut command = tokio::process::Command::new(program);
        command
            .args(args)
            .current_dir(&self.workdir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // 先套沙盒再写身份变量:沙盒会把 GIT_CONFIG_GLOBAL 指回真家的 ~/.gitconfig,
        // 后写的才作数。
        crate::tools::sandbox::confine(&mut command);
        apply_non_interactive(command.as_std_mut());
        if let Some(credentials) = bot {
            self.context
                .home
                .apply_bot(command.as_std_mut(), credentials);
        }
        tokio::time::timeout(STEP_TIMEOUT, command.output())
            .await
            .map_err(|_| {
                anyhow!(
                    "`{}` timed out after {}s",
                    command_label(program, args),
                    STEP_TIMEOUT.as_secs()
                )
            })?
            .with_context(|| format!("failed to run {program}. Is it installed and on PATH?"))
    }

    /// 以本次调用的身份跑一条命令,非零退出即报错。stderr 原样带回,模型要靠它
    /// 判断下一步。
    async fn run(&self, program: &str, args: &[&str]) -> Result<String> {
        checked(
            self.output(self.bot.as_ref(), program, args).await?,
            program,
            args,
        )
    }

    /// 永远以宿主身份跑。只用来读用户自己的 git 身份。
    async fn run_host(&self, program: &str, args: &[&str]) -> Result<String> {
        checked(self.output(None, program, args).await?, program, args)
    }

    async fn repo(&self, args: &Value) -> Result<String> {
        match optional_str(args, "repo") {
            Some(repo) => Ok(repo.to_string()),
            None => self
                .run(
                    "gh",
                    &[
                        "repo",
                        "view",
                        "--json",
                        "nameWithOwner",
                        "--jq",
                        ".nameWithOwner",
                    ],
                )
                .await
                .context("repo was not given and the working directory is not a GitHub repository"),
        }
    }
}

fn checked(output: std::process::Output, program: &str, args: &[&str]) -> Result<String> {
    let stdout = String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_string();
    if output.status.success() {
        return Ok(stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = if stderr.trim().is_empty() {
        stdout.as_str()
    } else {
        stderr.trim()
    };
    bail!(
        "`{}` failed ({}): {}",
        command_label(program, args),
        output.status,
        clip(detail)
    )
}

/// 报错里只带子命令,不带正文:commit 消息、PR 正文动辄几千字。
fn command_label(program: &str, args: &[&str]) -> String {
    let head: Vec<&str> = args.iter().take(2).copied().collect();
    format!("{program} {}", head.join(" "))
}

async fn status(context: &GithubContext, runner: &Runner<'_>) -> Result<String> {
    let host_login = runner
        .run_host("gh", &["api", "user", "--jq", ".login"])
        .await
        .map_err(|error| format!("{error:#}"));
    let account = context.home.account();
    let bot = match &account {
        None => json!({ "configured": false, "hint": "the user can run `gqy github login`" }),
        Some(account) => {
            let credentials = context
                .home
                .check_sandbox_access()
                .and_then(|()| context.home.credentials());
            let verified = match credentials {
                Err(error) => Err(format!("{error:#}")),
                Ok(credentials) => runner
                    .output(Some(&credentials), "gh", &["api", "user", "--jq", ".login"])
                    .await
                    .and_then(|output| checked(output, "gh", &["api", "user"]))
                    .map_err(|error| format!("{error:#}")),
            };
            json!({
                "configured": true,
                "login": account.login,
                "email": account.noreply_email(),
                "gh_login": verified.as_ref().ok(),
                "error": verified.as_ref().err(),
            })
        }
    };
    Ok(json!({
        "ok": true,
        "acting_identity": runner.identity.label(),
        "host_login": host_login.as_ref().ok(),
        "host_error": host_login.as_ref().err(),
        "bot": bot,
        "co_author_trailer": context.co_author().trailer(),
    })
    .to_string())
}

async fn commit(context: &GithubContext, runner: &Runner<'_>, args: &Value) -> Result<String> {
    let message = required_str(args, "message")?;
    if bool_arg(args, "add_all") {
        runner.run("git", &["add", "-A"]).await?;
    }
    let co_author = context.co_author();
    let message = append_trailer(message, &co_author);
    // 用户固定是 author。宿主身份下 git 自己就这么填;bot 身份下 committer 是
    // bot,author 得显式按用户自己的 git 配置填回去。
    let author = match runner.identity {
        Identity::Host => None,
        Identity::Bot => Some(host_author(runner).await?),
    };
    let mut git_args = vec!["commit", "-m", message.as_str()];
    if let Some(author) = &author {
        git_args.extend(["--author", author.as_str()]);
    }
    runner.run("git", &git_args).await?;
    let head = runner
        .run("git", &["log", "-1", "--format=%H%n%an <%ae>%n%cn <%ce>"])
        .await?;
    let mut lines = head.lines();
    Ok(json!({
        "ok": true,
        "identity": runner.identity.label(),
        "commit": lines.next(),
        "author": lines.next(),
        "committer": lines.next(),
        "co_author": co_author.trailer(),
    })
    .to_string())
}

async fn host_author(runner: &Runner<'_>) -> Result<String> {
    let name = runner.run_host("git", &["config", "user.name"]).await;
    let email = runner.run_host("git", &["config", "user.email"]).await;
    match (name, email) {
        (Ok(name), Ok(email)) if !name.trim().is_empty() && !email.trim().is_empty() => {
            Ok(format!("{} <{}>", name.trim(), email.trim()))
        }
        _ => bail!(
            "the commit author is unknown. The user's own git user.name and user.email must be set"
        ),
    }
}

async fn pr_create(context: &GithubContext, runner: &Runner<'_>, args: &Value) -> Result<String> {
    let title = required_str(args, "title")?;
    let body = optional_str(args, "body").unwrap_or("");
    let draft = args.get("draft").and_then(Value::as_bool).unwrap_or(true);
    let branch = match optional_str(args, "head") {
        Some(branch) => branch.to_string(),
        None => {
            runner
                .run("git", &["rev-parse", "--abbrev-ref", "HEAD"])
                .await?
        }
    };
    if branch == "HEAD" {
        bail!("HEAD is detached. Check out a branch or pass head");
    }
    let base = optional_str(args, "base");
    if base == Some(branch.as_str()) {
        bail!("head and base are the same branch ({branch}). Commit on a topic branch first");
    }
    let repo = runner.repo(args).await?;
    let refspec = format!("{branch}:refs/heads/{branch}");

    let head = if bool_arg(args, "fork") {
        let login = runner.run("gh", &["api", "user", "--jq", ".login"]).await?;
        // 已经 fork 过时 gh 只打印一句 already exists,照样零退出。
        runner
            .run(
                "gh",
                &["repo", "fork", &repo, "--clone=false", "--remote=false"],
            )
            .await?;
        let name = repo.rsplit('/').next().unwrap_or(&repo);
        let url = format!("https://github.com/{login}/{name}.git");
        push_with_retry(runner, &url, &refspec).await?;
        format!("{login}:{branch}")
    } else {
        let target = match runner.identity {
            Identity::Host => "origin".to_string(),
            // 不用 origin:它可能是 SSH 地址,bot 的凭据助手只管 HTTPS。
            Identity::Bot => format!("https://github.com/{repo}.git"),
        };
        runner.run("git", &["push", &target, &refspec]).await?;
        branch.clone()
    };

    let body = append_trailer(body, &context.co_author());
    let mut gh_args = vec![
        "pr", "create", "--repo", &repo, "--head", &head, "--title", title, "--body", &body,
    ];
    if let Some(base) = base {
        gh_args.extend(["--base", base]);
    }
    if draft {
        gh_args.push("--draft");
    }
    let output = runner.run("gh", &gh_args).await?;
    Ok(json!({
        "ok": true,
        "identity": runner.identity.label(),
        "url": output.lines().last(),
        "repo": repo,
        "head": head,
        "draft": draft,
    })
    .to_string())
}

/// 新建的 fork 要几秒才能接受 push。
async fn push_with_retry(runner: &Runner<'_>, url: &str, refspec: &str) -> Result<()> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        match runner.run("git", &["push", url, refspec]).await {
            Ok(_) => return Ok(()),
            Err(error) if attempt >= 3 => return Err(error),
            Err(_) => tokio::time::sleep(Duration::from_secs(3)).await,
        }
    }
}

async fn issue_create(
    context: &GithubContext,
    runner: &Runner<'_>,
    args: &Value,
) -> Result<String> {
    let title = required_str(args, "title")?;
    let body = append_trailer(
        optional_str(args, "body").unwrap_or(""),
        &context.co_author(),
    );
    let repo = runner.repo(args).await?;
    let output = runner
        .run(
            "gh",
            &[
                "issue", "create", "--repo", &repo, "--title", title, "--body", &body,
            ],
        )
        .await?;
    Ok(json!({
        "ok": true,
        "identity": runner.identity.label(),
        "url": output.lines().last(),
        "repo": repo,
    })
    .to_string())
}

async fn comment(runner: &Runner<'_>, args: &Value) -> Result<String> {
    let number = args
        .get("number")
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
        })
        .context("number is required: the issue or PR number")?;
    let body = required_str(args, "body")?;
    let repo = runner.repo(args).await?;
    // issues 评论接口对 PR 同样适用,一条路径两用。
    let endpoint = format!("repos/{repo}/issues/{number}/comments");
    let field = format!("body={body}");
    let url = runner
        .run(
            "gh",
            &[
                "api",
                &endpoint,
                "--method",
                "POST",
                "-f",
                &field,
                "--jq",
                ".html_url",
            ],
        )
        .await?;
    Ok(json!({
        "ok": true,
        "identity": runner.identity.label(),
        "url": url,
    })
    .to_string())
}

async fn passthrough(runner: &Runner<'_>, args: &Value) -> Result<String> {
    let gh_args = crate::tools::string_list(args.get("args"));
    if gh_args.is_empty() {
        bail!("args is required for the gh action: the arguments after gh, one per item");
    }
    let gh_args: Vec<&str> = gh_args.iter().map(String::as_str).collect();
    let output = runner.output(runner.bot.as_ref(), "gh", &gh_args).await?;
    // 非零退出不当工具错误:`gh pr checks` 有检查失败就退 8,输出才是答案。
    Ok(json!({
        "ok": output.status.success(),
        "identity": runner.identity.label(),
        "exit_code": output.status.code(),
        "stdout": clip(String::from_utf8_lossy(&output.stdout).trim_end()),
        "stderr": clip(String::from_utf8_lossy(&output.stderr).trim_end()),
    })
    .to_string())
}

fn required_str<'v>(args: &'v Value, key: &str) -> Result<&'v str> {
    optional_str(args, key).with_context(|| format!("{key} is required for this action"))
}

fn optional_str<'v>(args: &'v Value, key: &str) -> Option<&'v str> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn bool_arg(args: &Value, key: &str) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn clip(text: &str) -> String {
    if text.chars().count() <= MAX_OUTPUT_CHARS {
        return text.to_string();
    }
    let mut clipped: String = text.chars().take(MAX_OUTPUT_CHARS).collect();
    clipped.push_str("\n[output truncated]");
    clipped
}
