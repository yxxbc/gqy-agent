//! 后台任务的状态订阅与展示。
//!
//! 轮询线程（`spawn_jobs_poll_thread`）把 daemon 那边的任务状态拉过来，REPL 只
//! 读快照。`JOBS_FEED_MARK_LIMIT` 限制「已读」标记的数量——它只用于去重通知，
//! 无限增长毫无意义。

use crate::cli::*;

pub(in crate::cli) const JOB_SPINNER_FRAMES: [char; 10] =
    ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub(in crate::cli) fn format_job_duration(seconds: u64) -> String {
    if seconds >= 3600 {
        format!("{}h {:02}m", seconds / 3600, (seconds % 3600) / 60)
    } else if seconds >= 60 {
        format!("{}m {:02}s", seconds / 60, seconds % 60)
    } else {
        format!("{seconds}s")
    }
}

/// Status strip under the footer: a leading blank line, then one line per
/// background command with a blank line between entries. Timers are
/// right-aligned to the terminal width.
pub(in crate::cli) fn background_job_lines(
    jobs: &[crate::tools::jobs::JobOverview],
    spinner_phase: usize,
    cols: usize,
) -> Vec<String> {
    if jobs.is_empty() {
        return Vec::new();
    }
    let kind_label = |job: &crate::tools::jobs::JobOverview| match job.kind.as_str() {
        // 开发模式的子代理单列一类：那一条是去写代码的，「开发中」比「子代理」
        // 更说明它在干嘛。
        "dev" => crate::i18n::text("dev", "开发中"),
        "subagent" => crate::i18n::text("agent", "子代理"),
        _ => crate::i18n::text("cmd", "命令"),
    };
    // Pad kinds to one column so mixed command/subagent rows keep their ids
    // and titles vertically aligned.
    let kind_col = jobs
        .iter()
        .map(|job| visible_width(kind_label(job)))
        .max()
        .unwrap_or(0);
    let mut lines = vec![String::new()];
    for job in jobs.iter() {
        let marker = JOB_SPINNER_FRAMES[spinner_phase % JOB_SPINNER_FRAMES.len()];
        let kind_word = kind_label(job);
        let kind_pad = " ".repeat(kind_col.saturating_sub(visible_width(kind_word)));
        let mut left = format!(
            "{marker} {kind_word}{kind_pad} {} · {}",
            job.job_id, job.title
        );
        // 时间左边先报量：一条子代理跑五分钟，光有秒数看不出它是在干活还是
        // 卡住了（用户：这里时间左侧应该有一个 token 记述）。命令类任务没有
        // 这个概念，那儿就是空的。
        let timer = match job.metric.as_deref().filter(|text| !text.trim().is_empty()) {
            Some(metric) => format!(
                "{}  {}",
                metric.trim(),
                format_job_duration(job.runtime_seconds)
            ),
            None => format_job_duration(job.runtime_seconds),
        };
        let timer_width = visible_width(&timer);
        // Never exceed the terminal width: a wrapped strip line would shift
        // the whole tail and flicker.
        let max_left = cols.saturating_sub(timer_width).saturating_sub(2);
        while visible_width(&left) > max_left && !left.is_empty() {
            left.pop();
        }
        let left_width = visible_width(&left);
        let pad = cols
            .saturating_sub(left_width)
            .saturating_sub(timer_width)
            .max(1);
        lines.push(format!("\x1b[2m{left}{}{timer}\x1b[0m", " ".repeat(pad)));
    }
    lines
}

/// Strips the bracketed prefix off a background-job wake headline, leaving
/// `子代理完成 82bea3 · 标题`. The older `[后台命令完成] ` spelling still shows
/// up in sessions recorded before the rename.
pub(in crate::cli) fn job_wake_headline(headline: &str) -> String {
    headline
        .strip_prefix("[后台任务完成] ")
        .or_else(|| headline.strip_prefix("[后台命令完成] "))
        .map(str::to_string)
        .unwrap_or_else(|| headline.to_string())
}

/// Fires a desktop notification unless the REPL window has focus.
///
/// `focused` is `None` when there is no live tail — a one-shot `gqy ask` has
/// no window to be away from, so it stays quiet.
pub(in crate::cli) fn notify_if_unfocused(
    config: &AppConfig,
    focused: Option<bool>,
    title: &str,
    body: &str,
) {
    if !config.notifications.enabled || focused != Some(false) {
        return;
    }
    crate::notify::notify(title, &crate::notify::clip_body(body, 120));
}

/// Shared feed state between the remote REPL and its IPC poll thread.
#[derive(Default)]
pub(in crate::cli) struct SharedJobsFeed {
    /// The owning REPL's current session — strip snapshots are filtered to
    /// it (daemon "current session" can drift from the REPL's after /new).
    pub(in crate::cli) repl_session: std::sync::Mutex<Option<String>>,
    pub(in crate::cli) jobs: std::sync::Mutex<Vec<crate::tools::jobs::JobOverview>>,
    /// Rendered wake-turn reports waiting to be printed into the scrollback.
    pub(in crate::cli) reports: std::sync::Mutex<Vec<BackgroundReport>>,
    /// Latest session Σ read straight from the store. Background subagents
    /// bill to the session that launched them, but they finish long after the
    /// turn that spawned them published its totals — without this the footer
    /// sat on a stale Σ until the user happened to send another prompt.
    pub(in crate::cli) cumulative: std::sync::Mutex<Option<TurnTokens>>,
    /// 全部活跃回合（不只后台唤醒）：客户端按自己的会话过滤后跟播。
    pub(in crate::cli) live_runs: std::sync::Mutex<Vec<LiveRun>>,
    /// Runs already attached to (never re-follow), and turn ids that
    /// were rendered live (their DB report must not print again).
    pub(in crate::cli) followed_runs: std::sync::Mutex<std::collections::HashSet<String>>,
    pub(in crate::cli) rendered_turns: std::sync::Mutex<std::collections::HashSet<String>>,
}

/// 一条 daemon 侧正在跑的回合。`label` 只有后台唤醒有（"<job_id> · <title>"）；
/// 其余来源（WebUI / 另一个终端）在终端侧显示成通用标签。
#[derive(Debug, Clone, serde::Deserialize)]
pub(in crate::cli) struct LiveRun {
    pub(in crate::cli) run_id: String,
    #[serde(default)]
    pub(in crate::cli) session_id: String,
    #[serde(default)]
    pub(in crate::cli) label: Option<String>,
    #[serde(default)]
    pub(in crate::cli) turn_id: Option<String>,
}

/// 两个去重集合的容量兜底。常开 REPL 的后台唤醒一直发生,集合只增不减;
/// 死掉的 id 不会再被查到(run 不再出现在 live_runs、turn 已过水位线),
/// 超限时清掉无副作用。
pub(in crate::cli) const JOBS_FEED_MARK_LIMIT: usize = 4_096;

#[derive(Clone)]
pub(in crate::cli) struct BackgroundReport {
    pub(in crate::cli) turn_id: String,
    pub(in crate::cli) headline: String,
    pub(in crate::cli) reply: String,
}

/// Session isolation for the strip: keep only `session`'s jobs (sessionless
/// jobs stay visible as a legacy fallback; `None` session shows everything).
pub(in crate::cli) fn retain_session_jobs(
    jobs: &mut Vec<crate::tools::jobs::JobOverview>,
    session: Option<&str>,
) {
    if let Some(session) = session {
        jobs.retain(|job| job.session_id.is_none() || job.session_id.as_deref() == Some(session));
    }
}

/// Source of background-command snapshots for the idle status strip.
pub(in crate::cli) enum JobsFeed {
    /// Remote REPL: snapshots pushed by the IPC poll thread.
    Shared(std::sync::Arc<SharedJobsFeed>),
    /// Direct REPL: read the in-process registry, scoped to this REPL's
    /// session. 直连道以前直接读整张表不过滤——远端道在 poll 线程里过滤了，
    /// 两条路语义不一致，直连 REPL 会看到别的会话的后台命令。
    Local(Option<String>),
}

impl JobsFeed {
    pub(in crate::cli) fn current(&self) -> Vec<crate::tools::jobs::JobOverview> {
        match self {
            JobsFeed::Shared(shared) => shared.jobs.lock().unwrap().clone(),
            JobsFeed::Local(session) => {
                let mut jobs = crate::tools::jobs::overview();
                retain_session_jobs(&mut jobs, session.as_deref());
                jobs
            }
        }
    }

    /// The store's current Σ for the REPL's session, or `None` when this feed
    /// has no store behind it.
    pub(in crate::cli) fn cumulative(&self) -> Option<TurnTokens> {
        match self {
            JobsFeed::Shared(shared) => *shared.cumulative.lock().unwrap(),
            JobsFeed::Local(_) => None,
        }
    }

    pub(in crate::cli) fn take_reports(&self) -> Vec<BackgroundReport> {
        match self {
            JobsFeed::Shared(shared) => {
                let mut reports = shared.reports.lock().unwrap();
                let rendered = shared.rendered_turns.lock().unwrap();
                let taken = reports
                    .drain(..)
                    .filter(|report| !rendered.contains(&report.turn_id))
                    .collect();
                taken
            }
            JobsFeed::Local(_) => Vec::new(),
        }
    }

    /// Next live run in `session` that has not been followed yet; marks it
    /// followed so the caller attaches exactly once.
    ///
    /// 后台任务唤醒的回合和「同一条会话上别人（WebUI / 另一个终端）发起的
    /// 回合」走同一条通道：FollowRun + 同一个渲染器。自己发起的回合在发起时
    /// 就记进 `followed_runs`（见 [`Self::mark_own_run`]），不会被自己跟播。
    pub(in crate::cli) fn claim_live_run(&self, session: &str) -> Option<(String, String)> {
        let JobsFeed::Shared(shared) = self else {
            return None;
        };
        let live_runs = shared.live_runs.lock().unwrap();
        let mut followed = shared.followed_runs.lock().unwrap();
        for run in live_runs.iter() {
            if run.session_id != session || followed.contains(&run.run_id) {
                continue;
            }
            // turn 级去重：这个回合已经在别处（唤醒跟播）渲染过，run 可能还在
            // 收尾，不必再跟一遍。
            if run
                .turn_id
                .as_deref()
                .is_some_and(|turn_id| shared.rendered_turns.lock().unwrap().contains(turn_id))
            {
                followed.insert(run.run_id.clone());
                continue;
            }
            if followed.len() >= JOBS_FEED_MARK_LIMIT {
                followed.retain(|id| live_runs.iter().any(|run| &run.run_id == id));
            }
            followed.insert(run.run_id.clone());
            // 后台唤醒带自己的 label（"<job_id> · <title>"）；别的来源给通用
            // 标签——终端不外泄是谁发的（TurnOrigin 不分客户端）。
            let label = run
                .label
                .clone()
                .unwrap_or_else(|| crate::i18n::text("another client", "另一端").to_string());
            return Some((run.run_id.clone(), label));
        }
        None
    }

    /// 记下「这个 run 是本客户端自己发起的」：回合结束与轮询快照之间有秒级
    /// 窗口，不记的话刚跑完的回合会被自己当成「别人的回合」跟播一次空壳。
    pub(in crate::cli) fn mark_own_run(&self, run_id: &str) {
        let JobsFeed::Shared(shared) = self else {
            return;
        };
        let live_runs = shared.live_runs.lock().unwrap();
        let mut followed = shared.followed_runs.lock().unwrap();
        if followed.len() >= JOBS_FEED_MARK_LIMIT {
            followed.retain(|id| live_runs.iter().any(|run| &run.run_id == id));
        }
        followed.insert(run_id.to_string());
    }
}

/// Poll the daemon for background commands while the remote REPL idles:
/// 1s when commands are live, 3s when quiet — a unix-socket roundtrip
/// costs microseconds either way.
pub(in crate::cli) fn spawn_jobs_poll_thread(paths: GqyPaths) -> std::sync::Arc<SharedJobsFeed> {
    let shared = std::sync::Arc::new(SharedJobsFeed::default());
    let feed = shared.clone();
    std::thread::spawn(move || {
        let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        else {
            return;
        };
        // Track per-session watermarks so wake replies print exactly once,
        // and never replay history from before this REPL started.
        let mut seen: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        // The store open can lose a race against daemon writes (SQLITE_BUSY);
        // retry every cycle instead of deciding at startup forever.
        let mut store: Option<StateStore> = None;
        loop {
            if store.is_none() {
                store = StateStore::new(&paths).ok();
            }
            let (jobs, session_id, live_runs) = runtime
                .block_on(async {
                    tokio::time::timeout(
                        std::time::Duration::from_millis(500),
                        fetch_jobs_overview(&paths),
                    )
                    .await
                    .unwrap_or_else(|_| Ok((Vec::new(), None, Vec::new())))
                })
                .unwrap_or_default();
            let mut jobs = jobs;
            let repl_session = { feed.repl_session.lock().unwrap().clone() };
            retain_session_jobs(&mut jobs, repl_session.as_deref());
            *feed.jobs.lock().unwrap() = jobs;
            *feed.live_runs.lock().unwrap() = live_runs;
            if let (Some(store), Some(session)) = (store.as_ref(), repl_session.as_deref()) {
                if let Ok(totals) = store.pinned(session).session_cumulative_token_totals() {
                    *feed.cumulative.lock().unwrap() = Some(totals);
                }
            }
            if let (Some(store), Some(session_id)) = (store.as_ref(), session_id) {
                let watermark = match seen.entry(session_id.clone()) {
                    std::collections::hash_map::Entry::Occupied(entry) => *entry.get(),
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        let latest = store.latest_turn_seq(&session_id).unwrap_or(0);
                        *entry.insert(latest)
                    }
                };
                if let Ok(rows) = store.background_report_replies_after(&session_id, watermark) {
                    for (seq, turn_id, display, reply) in rows {
                        seen.insert(session_id.clone(), seq);
                        if feed.rendered_turns.lock().unwrap().contains(&turn_id) {
                            continue;
                        }
                        feed.reports.lock().unwrap().push(BackgroundReport {
                            turn_id,
                            headline: display,
                            reply,
                        });
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });
    shared
}

pub(in crate::cli) type JobsOverviewSnapshot = (
    Vec<crate::tools::jobs::JobOverview>,
    Option<String>,
    Vec<LiveRun>,
);

pub(in crate::cli) async fn fetch_jobs_overview(paths: &GqyPaths) -> Result<JobsOverviewSnapshot> {
    let mut stream = ipc::connect(&paths.ipc_socket()).await?;
    ipc::send(&mut stream, &IpcRequest::new(IpcCommand::JobsOverview)).await?;
    match ipc::receive::<IpcFrame>(&mut stream).await? {
        Some(IpcFrame::AdminResult { state, data }) => {
            // 老 daemon 只会给 wake_runs：解析不出来就是空表，退化成不跟播，
            // 不是错误。
            let live_runs = data
                .get("live_runs")
                .cloned()
                .map(serde_json::from_value::<Vec<LiveRun>>)
                .transpose()
                .unwrap_or_default()
                .unwrap_or_default();
            Ok((
                data.get("jobs")
                    .cloned()
                    .map(serde_json::from_value)
                    .transpose()
                    .unwrap_or_default()
                    .unwrap_or_default(),
                Some(state.session_id),
                live_runs,
            ))
        }
        _ => Ok((Vec::new(), None, Vec::new())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(run_id: &str, session: &str, turn: &str, label: Option<&str>) -> LiveRun {
        LiveRun {
            run_id: run_id.to_string(),
            session_id: session.to_string(),
            label: label.map(str::to_string),
            turn_id: Some(turn.to_string()),
        }
    }

    fn feed_with(runs: Vec<LiveRun>) -> JobsFeed {
        let shared = SharedJobsFeed::default();
        *shared.live_runs.lock().unwrap() = runs;
        JobsFeed::Shared(std::sync::Arc::new(shared))
    }

    /// 跟播只看自己的会话：认领按会话取，已经跟过的 run 不再重复认领。
    #[test]
    fn claim_live_run_is_scoped_to_the_repl_session_and_never_refollows() {
        let feed = feed_with(vec![
            run("run-other", "session-b", "turn-1", None),
            run("run-mine", "session-a", "turn-2", None),
        ]);
        let foreign = crate::i18n::text("another client", "另一端").to_string();
        assert_eq!(
            feed.claim_live_run("session-a"),
            Some(("run-mine".to_string(), foreign.clone()))
        );
        // 第二次是「已经跟过」，不该再发给调用方。
        assert_eq!(feed.claim_live_run("session-a"), None);
        // 别人的会话有自己的活跃回合；没有活跃回合的会话才什么都没有。
        assert_eq!(
            feed.claim_live_run("session-b"),
            Some(("run-other".to_string(), foreign))
        );
        assert_eq!(feed.claim_live_run("session-c"), None);
    }

    /// 后台唤醒保留自己的 label；别的来源（WebUI / 另一个终端）用通用标签。
    #[test]
    fn wake_runs_keep_their_label_and_foreign_runs_get_a_generic_one() {
        let feed = feed_with(vec![run(
            "run-wake",
            "session-a",
            "turn-1",
            Some("82bea3 · 跑测试"),
        )]);
        assert_eq!(
            feed.claim_live_run("session-a"),
            Some(("run-wake".to_string(), "82bea3 · 跑测试".to_string()))
        );
    }

    /// 已经在本客户端渲染过的回合不再跟播：run 可能还在收尾，跟了就只剩一个
    /// 空壳表头。
    #[test]
    fn claim_live_run_skips_turns_rendered_here() {
        let feed = feed_with(vec![run("run-1", "session-a", "turn-1", None)]);
        let JobsFeed::Shared(shared) = &feed else {
            unreachable!()
        };
        shared
            .rendered_turns
            .lock()
            .unwrap()
            .insert("turn-1".to_string());
        assert_eq!(feed.claim_live_run("session-a"), None);
    }

    /// 自己发起的回合先记名，免得回合结束与轮询快照之间的窗口把它当别人的。
    #[test]
    fn mark_own_run_keeps_the_repl_from_following_itself() {
        let feed = feed_with(vec![run("run-self", "session-a", "turn-1", None)]);
        feed.mark_own_run("run-self");
        assert_eq!(feed.claim_live_run("session-a"), None);
    }
}
