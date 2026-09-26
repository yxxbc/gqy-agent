//! 已连上的连接器。每个（平台, 账号）同时只留一条连接：连接器重连时新连接顶掉
//! 旧的，旧连接上没回的发送请求立刻失败，不会挂着等超时。

use super::protocol::{Capabilities, SendResult, ServerFrame};
use anyhow::{anyhow, Result};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tokio::sync::{mpsc, oneshot, watch};

/// 纯文字发送等回执的上限。连接器那边调 osascript 之类，通常一两秒。
pub(crate) const TEXT_SEND_TIMEOUT: Duration = Duration::from_secs(30);
/// 带附件的发送：上传大图可能要一阵子，这只是防连接器装死的兜底。
pub(crate) const ATTACHMENT_SEND_TIMEOUT: Duration = Duration::from_secs(180);

/// 每个对话攒着的点按回应最多留几条（下一轮对话一并带上）。
const MAX_PENDING_NOTES: usize = 10;

/// 最近处理完的事件 id 记多少个。连接器重连后可能重发还没收到 ack 的事件，
/// 在这个窗口里的直接回 ack，不再跑一遍回合。
const RECENT_EVENT_IDS: usize = 512;

#[derive(Clone)]
pub(crate) struct ConnectorHandle {
    pub(crate) id: u64,
    pub(crate) platform: String,
    pub(crate) account: String,
    pub(crate) display_name: String,
    pub(crate) connector_name: String,
    pub(crate) connector_version: String,
    pub(crate) capabilities: Capabilities,
    pub(crate) connected_at: SystemTime,
    out_tx: mpsc::UnboundedSender<String>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<SendResult>>>>,
    next_req: Arc<AtomicU64>,
    shutdown: watch::Sender<bool>,
}

impl ConnectorHandle {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        id: u64,
        platform: String,
        account: String,
        display_name: String,
        connector_name: String,
        connector_version: String,
        capabilities: Capabilities,
        out_tx: mpsc::UnboundedSender<String>,
    ) -> (Self, watch::Receiver<bool>) {
        let (shutdown, shutdown_rx) = watch::channel(false);
        (
            Self {
                id,
                platform,
                account,
                display_name,
                connector_name,
                connector_version,
                capabilities,
                connected_at: SystemTime::now(),
                out_tx,
                pending: Arc::new(Mutex::new(HashMap::new())),
                next_req: Arc::new(AtomicU64::new(1)),
                shutdown,
            },
            shutdown_rx,
        )
    }

    pub(crate) fn send_frame(&self, frame: &ServerFrame<'_>) -> Result<()> {
        self.out_tx
            .send(frame.encode())
            .map_err(|_| anyhow!("the {} connector disconnected", self.platform))
    }

    /// 发一个 `send` 帧并等 `send_result`。
    pub(crate) async fn request(
        &self,
        build: impl FnOnce(u64) -> String,
        timeout: Duration,
    ) -> Result<SendResult> {
        let req = self.next_req.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(req, tx);
        if self.out_tx.send(build(req)).is_err() {
            self.pending.lock().unwrap().remove(&req);
            return Err(anyhow!("the {} connector disconnected", self.platform));
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(_)) => Err(anyhow!(
                "the {} connector disconnected before confirming the send",
                self.platform
            )),
            Err(_) => {
                self.pending.lock().unwrap().remove(&req);
                Err(anyhow!(
                    "the {} connector did not confirm the send within {}s",
                    self.platform,
                    timeout.as_secs()
                ))
            }
        }
    }

    pub(crate) fn route_result(&self, result: SendResult) {
        if let Some(tx) = self.pending.lock().unwrap().remove(&result.req) {
            let _ = tx.send(result);
        }
    }

    /// 连接结束：丢掉所有等待中的回执通道，等待方立刻拿到「已断开」。
    pub(crate) fn fail_pending(&self) {
        self.pending.lock().unwrap().clear();
    }

    pub(crate) fn disconnect(&self) {
        let _ = self.shutdown.send(true);
    }
}

#[derive(Default)]
struct Inner {
    next_id: u64,
    connections: HashMap<u64, ConnectorHandle>,
    /// 每个平台正在处理的事件 id（收到后、ack 前）。
    in_flight: HashMap<String, HashSet<String>>,
    /// 每个平台最近处理完的事件 id。
    recent: HashMap<String, VecDeque<String>>,
    /// 按对话（`PlatformConversation::scope_key`）攒的点按回应说明。
    notes: HashMap<String, VecDeque<String>>,
}

#[derive(Clone, Default)]
pub(crate) struct ConnectorRegistry {
    inner: Arc<Mutex<Inner>>,
}

/// 一条事件该怎么处理。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum EventAdmission {
    /// 第一次见，去处理。
    New,
    /// 已经处理完了（连接器没收到 ack 又发了一次），直接回 ack。
    AlreadyDone,
    /// 正在处理，处理完会回 ack，这次忽略。
    InFlight,
}

impl ConnectorRegistry {
    pub(crate) fn next_connection_id(&self) -> u64 {
        let mut inner = self.inner.lock().unwrap();
        inner.next_id += 1;
        inner.next_id
    }

    /// 登记新连接，同平台同账号的旧连接断开。
    pub(crate) fn register(&self, handle: ConnectorHandle) {
        let replaced = {
            let mut inner = self.inner.lock().unwrap();
            let replaced: Vec<ConnectorHandle> = inner
                .connections
                .values()
                .filter(|existing| {
                    existing.platform == handle.platform && existing.account == handle.account
                })
                .cloned()
                .collect();
            for old in &replaced {
                inner.connections.remove(&old.id);
            }
            inner.connections.insert(handle.id, handle);
            replaced
        };
        for old in replaced {
            old.fail_pending();
            old.disconnect();
        }
    }

    pub(crate) fn remove(&self, id: u64) {
        let removed = self.inner.lock().unwrap().connections.remove(&id);
        if let Some(handle) = removed {
            handle.fail_pending();
        }
    }

    /// 平台（与账号）当前的连接。账号留空 = 该平台任意一条。
    pub(crate) fn handle(&self, platform: &str, account: &str) -> Option<ConnectorHandle> {
        let inner = self.inner.lock().unwrap();
        inner
            .connections
            .values()
            .filter(|handle| handle.platform == platform)
            .filter(|handle| account.is_empty() || handle.account == account)
            .max_by_key(|handle| handle.id)
            .cloned()
    }

    pub(crate) fn connected(&self) -> Vec<ConnectorHandle> {
        let mut handles: Vec<ConnectorHandle> = self
            .inner
            .lock()
            .unwrap()
            .connections
            .values()
            .cloned()
            .collect();
        handles.sort_by(|a, b| (&a.platform, &a.account).cmp(&(&b.platform, &b.account)));
        handles
    }

    /// 断开满足条件的连接（配置改了：平台关掉、口令换了）。
    pub(crate) fn disconnect_where(&self, predicate: impl Fn(&ConnectorHandle) -> bool) {
        let handles: Vec<ConnectorHandle> = self
            .inner
            .lock()
            .unwrap()
            .connections
            .values()
            .filter(|handle| predicate(handle))
            .cloned()
            .collect();
        for handle in handles {
            handle.fail_pending();
            handle.disconnect();
        }
    }

    pub(crate) fn admit_event(&self, platform: &str, event_id: &str) -> EventAdmission {
        let mut inner = self.inner.lock().unwrap();
        if inner
            .recent
            .get(platform)
            .is_some_and(|recent| recent.iter().any(|id| id == event_id))
        {
            return EventAdmission::AlreadyDone;
        }
        let in_flight = inner.in_flight.entry(platform.to_string()).or_default();
        if in_flight.insert(event_id.to_string()) {
            EventAdmission::New
        } else {
            EventAdmission::InFlight
        }
    }

    /// 点按回应不单独开回合，记下来等下一条消息一起给她看。
    pub(crate) fn push_note(&self, scope: &str, note: String) {
        let mut inner = self.inner.lock().unwrap();
        let notes = inner.notes.entry(scope.to_string()).or_default();
        notes.push_back(note);
        while notes.len() > MAX_PENDING_NOTES {
            notes.pop_front();
        }
    }

    pub(crate) fn take_notes(&self, scope: &str) -> Vec<String> {
        self.inner
            .lock()
            .unwrap()
            .notes
            .remove(scope)
            .map(Vec::from)
            .unwrap_or_default()
    }

    pub(crate) fn finish_event(&self, platform: &str, event_id: &str) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(in_flight) = inner.in_flight.get_mut(platform) {
            in_flight.remove(event_id);
        }
        let recent = inner.recent.entry(platform.to_string()).or_default();
        recent.push_back(event_id.to_string());
        while recent.len() > RECENT_EVENT_IDS {
            recent.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handle(
        registry: &ConnectorRegistry,
        account: &str,
    ) -> (ConnectorHandle, watch::Receiver<bool>) {
        let (tx, _rx) = mpsc::unbounded_channel();
        ConnectorHandle::new(
            registry.next_connection_id(),
            "imessage".into(),
            account.into(),
            "iMessage".into(),
            "test".into(),
            "0".into(),
            Capabilities::default(),
            tx,
        )
    }

    #[test]
    fn reconnect_replaces_the_old_connection() {
        let registry = ConnectorRegistry::default();
        let (first, first_shutdown) = handle(&registry, "");
        registry.register(first.clone());
        let (second, _) = handle(&registry, "");
        registry.register(second.clone());
        assert!(*first_shutdown.borrow());
        assert_eq!(registry.handle("imessage", "").unwrap().id, second.id);
        assert_eq!(registry.connected().len(), 1);
    }

    #[test]
    fn replayed_events_are_acked_without_running_twice() {
        let registry = ConnectorRegistry::default();
        assert_eq!(registry.admit_event("imessage", "1"), EventAdmission::New);
        assert_eq!(
            registry.admit_event("imessage", "1"),
            EventAdmission::InFlight
        );
        registry.finish_event("imessage", "1");
        assert_eq!(
            registry.admit_event("imessage", "1"),
            EventAdmission::AlreadyDone
        );
        assert_eq!(registry.admit_event("telegram", "1"), EventAdmission::New);
    }
}
