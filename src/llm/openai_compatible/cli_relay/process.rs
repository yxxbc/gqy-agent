//! CLI 中转子进程的生命周期:拉起、喂 stdin、按行读 stdout(带空闲看门狗)、
//! 收 stderr 尾巴、收尾等待/击杀。三条线的事件语法各不相同,但进程这一层
//! 完全一样——尤其是「超时/出错必须显式杀进程组:drop future 只是弃 promise,
//! 不杀子进程」这条,三处各抄一遍就会有一处漏。

use crate::llm::openai_compatible::*;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdout};

pub(in crate::llm::openai_compatible) fn kill_process_group(pid: u32) {
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
}

pub(in crate::llm::openai_compatible) struct RelayProcess {
    child: Child,
    pid: u32,
    lines: Lines<BufReader<ChildStdout>>,
    stderr_tail: Arc<Mutex<String>>,
    stderr_task: tokio::task::JoinHandle<()>,
    /// stdin 写入在独立任务里进行:先读 stdout 再等写完。CLI 在读 stdin 之前
    /// 就退出(续传目标丢失、登录失败)时,大于管道缓冲的载荷会让同步 write_all
    /// 永远等不到人读,或者拿到一个没有 stderr 尾巴的 EPIPE——两种都盖住了
    /// 真正的报错措辞(评审 09-03)。
    stdin_task: Option<tokio::task::JoinHandle<()>>,
    /// 预热进程先拉起、后喂载荷,写端在这里存着等 [`RelayProcess::send_payload`]。
    stdin: Option<tokio::process::ChildStdin>,
    idle_timeout: Duration,
    /// 看门狗报错里的阶段名(`claude-code.stream` 这种)。
    stage: &'static str,
    label: &'static str,
}

/// 三条中转线各自的配置/登录态目录:claude(`~/.claude`、`~/.claude.json`)、
/// codex(`~/.codex`)、agy(`~/.gemini`,或 `GQY_AGY_CONFIG_DIR`)。不存在的不给。
fn relay_config_grants() -> Vec<std::path::PathBuf> {
    let mut grants = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        let home = std::path::PathBuf::from(home);
        for name in [".claude", ".claude.json", ".codex", ".gemini"] {
            grants.push(home.join(name));
        }
    }
    if let Some(dir) = std::env::var_os("GQY_AGY_CONFIG_DIR") {
        grants.push(std::path::PathBuf::from(dir));
    }
    grants
}

impl RelayProcess {
    /// 拉起子进程并把整段 stdin 载荷写完、关写端(本轮输入结束)。
    /// `env` 里 `None` 表示从子进程环境里抹掉该变量。
    #[allow(clippy::too_many_arguments)]
    pub(in crate::llm::openai_compatible) async fn spawn(
        binary: &std::path::Path,
        args: &[String],
        workdir: &std::path::Path,
        env: &[(String, Option<String>)],
        stdin_payload: &str,
        idle_timeout: Duration,
        stage: &'static str,
        label: &'static str,
        not_found: impl FnOnce() -> String,
    ) -> Result<Self> {
        let mut process = Self::spawn_idle(
            binary,
            args,
            workdir,
            env,
            idle_timeout,
            stage,
            label,
            not_found,
        )
        .await?;
        process.send_payload(stdin_payload);
        Ok(process)
    }

    /// 只拉起进程,不喂 stdin。预热用:CLI 在收到输入之前就会把登录、MCP 握手
    /// 这些固定开销跑完(agy 实测 4.5 秒),载荷晚点再给也不影响。
    #[allow(clippy::too_many_arguments)]
    pub(in crate::llm::openai_compatible) async fn spawn_idle(
        binary: &std::path::Path,
        args: &[String],
        workdir: &std::path::Path,
        env: &[(String, Option<String>)],
        idle_timeout: Duration,
        stage: &'static str,
        label: &'static str,
        not_found: impl FnOnce() -> String,
    ) -> Result<Self> {
        let mut command = tokio::process::Command::new(binary);
        command
            .args(args)
            .current_dir(workdir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .process_group(0)
            .kill_on_drop(true);
        for (key, value) in env {
            match value {
                Some(value) => {
                    command.env(key, value);
                }
                None => {
                    command.env_remove(key);
                }
            }
        }
        // 沙盒回合(成员):CLI 进程整个关进 Landlock,它自带的 Bash/Edit 子进程一并
        // 继承;CLI 自己的配置目录(登录态、会话文件)放行读写。
        crate::tools::sandbox::confine_relay(&mut command, &relay_config_grants());
        let mut child = command.spawn().map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                anyhow::anyhow!("{}: {}", not_found(), binary.display())
            } else {
                anyhow::Error::from(error).context(format!("failed to spawn {label}"))
            }
        })?;
        let pid = child.id().unwrap_or_default();
        let stdin = child
            .stdin
            .take()
            .with_context(|| format!("{label} stdin unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .with_context(|| format!("{label} stdout unavailable"))?;
        let stderr = child
            .stderr
            .take()
            .with_context(|| format!("{label} stderr unavailable"))?;
        // stderr 尾巴单独收,失败时并进报错(限流/登录错误常常只在 stderr)。
        let stderr_tail = Arc::new(Mutex::new(String::new()));
        let stderr_task = {
            let tail = stderr_tail.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Ok(mut tail) = tail.lock() {
                        tail.push_str(&line);
                        tail.push('\n');
                        let overflow = tail.len().saturating_sub(8192);
                        if overflow > 0 {
                            let cut = tail
                                .char_indices()
                                .map(|(index, _)| index)
                                .find(|index| *index >= overflow)
                                .unwrap_or(0);
                            tail.drain(..cut);
                        }
                    }
                }
            })
        };
        Ok(Self {
            child,
            pid,
            lines: BufReader::new(stdout).lines(),
            stderr_tail,
            stderr_task,
            stdin_task: None,
            stdin: Some(stdin),
            idle_timeout,
            stage,
            label,
        })
    }

    /// 把本轮载荷写进 stdin 并关写端(本轮输入结束)。写在独立任务里:见
    /// `stdin_task` 字段上的说明。
    pub(in crate::llm::openai_compatible) fn send_payload(&mut self, stdin_payload: &str) {
        let Some(mut stdin) = self.stdin.take() else {
            return;
        };
        let payload = stdin_payload.as_bytes().to_vec();
        let label = self.label;
        self.stdin_task = Some(tokio::spawn(async move {
            if let Err(error) = stdin.write_all(&payload).await {
                // 子进程先退出(EPIPE)属正常:真正的原因在 stdout/stderr 里。
                tracing::debug!(%error, "{label} closed stdin before the payload was written");
            }
            drop(stdin);
        }));
    }

    /// 下一行 stdout;空闲超过看门狗就杀进程组并报 Timeout 类传输失败。
    /// `Ok(None)` = stdout 关闭(进程退出)。
    pub(in crate::llm::openai_compatible) async fn next_line(&mut self) -> Result<Option<String>> {
        match tokio::time::timeout(self.idle_timeout, self.lines.next_line()).await {
            Err(_) => {
                self.kill();
                Err(anyhow::anyhow!(
                    "{} produced no output for {}s; the process was killed",
                    self.label,
                    self.idle_timeout.as_secs()
                )
                .context(TransportFailure {
                    stage: self.stage,
                    kind: TransportFailureKind::Timeout,
                }))
            }
            Ok(read) => read.with_context(|| format!("failed to read {} stdout", self.label)),
        }
    }

    pub(in crate::llm::openai_compatible) fn kill(&self) {
        kill_process_group(self.pid);
    }

    pub(in crate::llm::openai_compatible) fn stderr_tail(&self) -> String {
        self.stderr_tail
            .lock()
            .map(|tail| tail.trim().to_string())
            .unwrap_or_default()
    }

    /// 终态帧到手后进程应当自然退出;不给它耗着的机会。返回退出码文本。
    pub(in crate::llm::openai_compatible) async fn finish(mut self) -> (String, String) {
        let exit = match tokio::time::timeout(Duration::from_secs(10), self.child.wait()).await {
            Ok(status) => status.ok(),
            Err(_) => {
                self.kill();
                None
            }
        };
        self.stderr_task.abort();
        if let Some(task) = &self.stdin_task {
            task.abort();
        }
        let code = exit
            .and_then(|status| status.code())
            .map(|code| code.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        (code, self.stderr_tail())
    }
}
