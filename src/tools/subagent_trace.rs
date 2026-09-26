//! 子代理过程标记的合并与落库。
//!
//! 子代理的思考与正文是逐 token 上来的，一个 token 一条标记。原样存的话，一段
//! 长思考就能把 4000 条的缓冲撑满，从头丢掉前面的工具步骤——回看时开头没了。
//! 这里把相邻的同类增量并成一段：渲染端本来就是把连续增量累加进同一块，合并
//! 前后画出来一模一样。
//!
//! `TraceRecorder` 把合并后的标记追加进审计会话的 `subagent_trace` 子表，
//! WebUI 的详情抽屉从那里读完整过程，不受内存缓冲寿命与 daemon 重启影响。

use crate::state::StateStore;
use std::sync::Mutex;

pub(crate) const REASONING_PREFIX: &str = "__subagent_reasoning__";
pub(crate) const CONTENT_PREFIX: &str = "__subagent_content__";
pub(crate) const METRIC_PREFIX: &str = "__subagent_metric__";
pub(crate) const PREPARING_PREFIX: &str = "__subtool_preparing__";
/// 审计会话 id。WebUI 据它打开详情抽屉；终端渲染器直接丢掉。
pub(crate) const SESSION_PREFIX: &str = "__subagent_session__";

/// 一段合并文本攒到多长就先落一条。续上的同类标记在渲染端接着累加进同一块，
/// 切开不影响画面，只是让中途断掉时丢得少一点。
const SEGMENT_FLUSH_CHARS: usize = 4000;

/// 逐 token 的增量：返回（前缀, 文本）。
fn delta_parts(marker: &str) -> Option<(&'static str, &str)> {
    for prefix in [REASONING_PREFIX, CONTENT_PREFIX] {
        if let Some(text) = marker.strip_prefix(prefix) {
            return Some((prefix, text));
        }
    }
    None
}

/// 往内存缓冲追加一条标记：同类增量接在上一条后面，中途量报只留最新一条。
pub(crate) fn push_coalesced(buffer: &mut Vec<String>, marker: &str) {
    if let Some((prefix, text)) = delta_parts(marker) {
        if let Some(last) = buffer.last_mut() {
            if last.starts_with(prefix) {
                last.push_str(text);
                return;
            }
        }
    } else if marker.starts_with(METRIC_PREFIX) {
        if let Some(last) = buffer.last_mut() {
            if last.starts_with(METRIC_PREFIX) {
                *last = marker.to_string();
                return;
            }
        }
    }
    buffer.push(marker.to_string());
}

/// 边跑边把合并后的标记写进审计会话。写库失败只记日志，不影响子代理本身。
pub(crate) struct TraceRecorder {
    store: StateStore,
    session_id: String,
    pending: Mutex<Option<String>>,
}

impl TraceRecorder {
    pub(crate) fn new(store: StateStore, session_id: String) -> Self {
        Self {
            store,
            session_id,
            pending: Mutex::new(None),
        }
    }

    pub(crate) fn push(&self, marker: &str) {
        // 量报一秒好几次、准备行一闪就过：都不是过程的一部分，用量另有落账。
        if marker.starts_with(METRIC_PREFIX) || marker.starts_with(PREPARING_PREFIX) {
            return;
        }
        let mut pending = self.pending.lock().unwrap();
        if let Some((prefix, text)) = delta_parts(marker) {
            let same_kind = pending
                .as_ref()
                .is_some_and(|segment| segment.starts_with(prefix));
            if same_kind {
                if let Some(segment) = pending.as_mut() {
                    segment.push_str(text);
                }
            } else {
                let previous = pending.replace(marker.to_string());
                self.write(previous);
            }
            let long = pending
                .as_ref()
                .is_some_and(|segment| segment.len() > SEGMENT_FLUSH_CHARS);
            if long {
                self.write(pending.take());
            }
            return;
        }
        let previous = pending.take();
        let mut rows: Vec<String> = previous.into_iter().collect();
        rows.push(marker.to_string());
        self.write_rows(&rows);
    }

    /// 收尾：没等到下一个事件的那段文本也落下。
    pub(crate) fn flush(&self) {
        let segment = self.pending.lock().unwrap().take();
        self.write(segment);
    }

    fn write(&self, segment: Option<String>) {
        if let Some(segment) = segment {
            self.write_rows(&[segment]);
        }
    }

    fn write_rows(&self, rows: &[String]) {
        if let Err(error) = self.store.append_subagent_trace(&self.session_id, rows) {
            tracing::warn!(error = %error, "failed to record subagent trace");
        }
    }
}

impl Drop for TraceRecorder {
    fn drop(&mut self) {
        self.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::TurnStatus;

    #[test]
    fn deltas_of_one_kind_merge_into_one_marker() {
        let mut buffer = Vec::new();
        for marker in [
            "__subagent_brief__{}",
            "__subagent_reasoning__Let ",
            "__subagent_reasoning__me look",
            "__subtool_call__{\"name\":\"read\"}",
            "__subagent_content__Done",
            "__subagent_content__.",
            "__subagent_reasoning__again",
        ] {
            push_coalesced(&mut buffer, marker);
        }
        assert_eq!(
            buffer,
            vec![
                "__subagent_brief__{}",
                "__subagent_reasoning__Let me look",
                "__subtool_call__{\"name\":\"read\"}",
                "__subagent_content__Done.",
                "__subagent_reasoning__again",
            ]
        );
    }

    #[test]
    fn consecutive_metrics_keep_only_the_latest() {
        let mut buffer = Vec::new();
        push_coalesced(&mut buffer, "__subagent_metric__1\t1\ta");
        push_coalesced(&mut buffer, "__subagent_metric__2\t2\tb");
        push_coalesced(&mut buffer, "__subtool_result__{}");
        push_coalesced(&mut buffer, "__subagent_metric__3\t3\tc");
        assert_eq!(
            buffer,
            vec![
                "__subagent_metric__2\t2\tb",
                "__subtool_result__{}",
                "__subagent_metric__3\t3\tc",
            ]
        );
    }

    /// 落库这一半：合并后写进审计会话、跑着时读得到已落的部分、跟着会话级联删除。
    #[test]
    fn recorder_merges_deltas_and_detail_reads_them_back() {
        let temp = tempfile::tempdir().unwrap();
        let paths = crate::tools::tests::test_paths(temp.path());
        let store = StateStore::new(&paths).unwrap();
        store.init_files().unwrap();
        let audit = store
            .create_session("gqy", "查目录", "subagent", None)
            .unwrap();
        let pinned = store.pinned(&audit.session_id);
        pinned
            .start_turn("sat_1", "列出 src 下的文件", std::process::id())
            .unwrap();

        let recorder = TraceRecorder::new(store.clone(), audit.session_id.clone());
        for marker in [
            "__subagent_brief__{\"description\":\"查目录\"}",
            "__subagent_reasoning__先",
            "__subagent_reasoning__看看",
            "__subagent_metric__≈1\t1\t工具调用 0 次",
            "__subtool_preparing__run_command",
            "__subtool_call__{\"name\":\"run_command\"}",
            "__subtool_result__{\"name\":\"run_command\",\"ok\":true}",
            "__subagent_content__好",
            "__subagent_content__了",
        ] {
            recorder.push(marker);
        }
        // 跑着的时候读:最后那段正文还没等到下一个事件,不在库里。
        let running = store
            .subagent_audit_detail(&audit.session_id)
            .unwrap()
            .unwrap();
        assert_eq!(running.status, TurnStatus::Running);
        assert_eq!(
            store.subagent_trace(&audit.session_id).unwrap().len(),
            4,
            "量报与准备行不进过程,两段思考增量并成一条"
        );
        recorder.flush();
        pinned
            .complete_turn(
                "sat_1",
                "subagent completed (tier standard): 查目录\nstats: {}\nresult:\n好了",
                None,
            )
            .unwrap();

        assert_eq!(
            store.subagent_trace(&audit.session_id).unwrap(),
            vec![
                "__subagent_brief__{\"description\":\"查目录\"}",
                "__subagent_reasoning__先看看",
                "__subtool_call__{\"name\":\"run_command\"}",
                "__subtool_result__{\"name\":\"run_command\",\"ok\":true}",
                "__subagent_content__好了",
            ]
        );
        let detail = store
            .subagent_audit_detail(&audit.session_id)
            .unwrap()
            .unwrap();
        assert_eq!(detail.status, TurnStatus::Completed);
        assert_eq!(detail.prompt, "列出 src 下的文件");
        assert!(detail.output.ends_with("result:\n好了"));
        assert_eq!(detail.record.name, "查目录");

        // 不是子代理审计会话的,一律当不存在。
        let user = store.create_session("gqy", "聊天", "user", None).unwrap();
        assert!(store
            .subagent_audit_detail(&user.session_id)
            .unwrap()
            .is_none());

        // 审计会话过了保留期被清掉时,过程跟着级联删除。
        {
            use rusqlite::params;
            let db_path = paths.state_dir.join("conversation.db");
            let conn = rusqlite::Connection::open(db_path).unwrap();
            let backdated = (chrono::Utc::now() - chrono::Duration::days(10)).to_rfc3339();
            conn.execute(
                "UPDATE sessions SET updated_at = ?1 WHERE session_id = ?2",
                params![backdated, audit.session_id],
            )
            .unwrap();
        }
        assert_eq!(store.delete_subagent_sessions_older_than(7).unwrap(), 1);
        assert!(store.subagent_trace(&audit.session_id).unwrap().is_empty());
    }
}
