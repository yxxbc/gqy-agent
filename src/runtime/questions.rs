//! 提问代理：把模型的提问送到前端，等回答再送回来。
// 兄弟模块的类型互相引用（DaemonState 持有 EventHub、run 记录引用
// ManagerState 等），统一从 mod.rs 的再导出取，免得每个文件维护一份
// 交叉导入清单。
use super::*;
use crate::question::{QuestionAnswers, QuestionRequest, QuestionResponse};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

// ── QuestionBroker 提问代理 ──
#[derive(Clone)]
pub(crate) struct QuestionBroker {
    pub(crate) pending: Arc<Mutex<HashMap<String, PendingQuestion>>>,
    /// 最近被解决的提问留档。答完即从 `pending` 删（`AnswerFailure::NotFound`），
    /// 光看它查不出「刚才被答过」——面板一侧要靠这份留档把「已在别处处理」同
    /// 「还在等」区分开。
    resolved: Arc<Mutex<VecDeque<(String, QuestionResolution)>>>,
}

pub(crate) struct PendingQuestion {
    pub(crate) run_id: String,
    pub(crate) request: QuestionRequest,
    pub(crate) responder: oneshot::Sender<QuestionResponse>,
}

/// 一条提问的终局。
#[derive(Clone)]
pub(crate) enum QuestionResolution {
    Answered(QuestionAnswers),
    Closed,
    Cancelled,
}

/// `QuestionState` 查询的答复。
pub(crate) enum QuestionStateView {
    Pending,
    Answered(QuestionAnswers),
    Closed,
    Cancelled,
    /// 既不在等、也没有留档：从来没问过，或者 daemon 重启后留档丢了。
    Unknown,
}

impl QuestionStateView {
    pub(crate) fn state_name(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Answered(_) => "answered",
            Self::Closed => "closed",
            Self::Cancelled => "cancelled",
            Self::Unknown => "unknown",
        }
    }

    pub(crate) fn answers(&self) -> Option<&QuestionAnswers> {
        match self {
            Self::Answered(answers) => Some(answers),
            _ => None,
        }
    }
}

/// 留档上限。查询窗口只有「面板开着」的那段时间，128 条足够；满了丢最老的，
/// 不给常驻 daemon 添一张无界增长的表。
const RESOLVED_HISTORY: usize = 128;

#[derive(Debug)]
pub(crate) enum AnswerFailure {
    NotFound,
    Invalid(String),
    Gone,
}

impl QuestionBroker {
    pub(crate) fn new() -> Self {
        Self {
            pending: Arc::new(Mutex::new(HashMap::new())),
            resolved: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// 记一条终局。满了丢最老的——留档只服务于面板查询，丢老的没有副作用。
    fn remember(&self, question_id: &str, resolution: QuestionResolution) {
        let mut resolved = self.resolved.lock().unwrap();
        resolved.push_back((question_id.to_string(), resolution));
        while resolved.len() > RESOLVED_HISTORY {
            resolved.pop_front();
        }
    }

    /// 这个提问现在是什么状态（面板开着时按它轮询）。
    pub(crate) fn state(&self, question_id: &str) -> QuestionStateView {
        if self.pending.lock().unwrap().contains_key(question_id) {
            return QuestionStateView::Pending;
        }
        let resolved = self.resolved.lock().unwrap();
        match resolved
            .iter()
            .rev()
            .find(|(id, _)| id == question_id)
            .map(|(_, resolution)| resolution)
        {
            Some(QuestionResolution::Answered(answers)) => {
                QuestionStateView::Answered(answers.clone())
            }
            Some(QuestionResolution::Closed) => QuestionStateView::Closed,
            Some(QuestionResolution::Cancelled) => QuestionStateView::Cancelled,
            None => QuestionStateView::Unknown,
        }
    }

    pub(crate) fn insert(
        &self,
        run_id: &str,
        request: QuestionRequest,
        responder: oneshot::Sender<QuestionResponse>,
    ) -> String {
        let mut pending = self.pending.lock().unwrap();
        loop {
            let question_id = random_id("question", 18);
            if !pending.contains_key(&question_id) {
                pending.insert(
                    question_id.clone(),
                    PendingQuestion {
                        run_id: run_id.to_string(),
                        request,
                        responder,
                    },
                );
                return question_id;
            }
        }
    }

    pub(crate) fn answer<F>(
        &self,
        question_id: &str,
        answers: QuestionAnswers,
        before_resume: F,
    ) -> std::result::Result<(), AnswerFailure>
    where
        F: FnOnce(&str, &QuestionAnswers),
    {
        let mut all_pending = self.pending.lock().unwrap();
        let request = all_pending
            .get(question_id)
            .map(|pending| pending.request.clone())
            .ok_or(AnswerFailure::NotFound)?;
        let answers = normalize_answers(&request, answers).map_err(AnswerFailure::Invalid)?;
        let pending = all_pending
            .remove(question_id)
            .ok_or(AnswerFailure::NotFound)?;
        let run_id = pending.run_id;
        if pending.responder.is_closed() {
            return Err(AnswerFailure::Gone);
        }
        before_resume(&run_id, &answers);
        if pending
            .responder
            .send(QuestionResponse::Answered(answers.clone()))
            .is_err()
        {
            return Err(AnswerFailure::Gone);
        }
        self.remember(question_id, QuestionResolution::Answered(answers));
        Ok(())
    }

    pub(crate) fn close<F>(
        &self,
        question_id: &str,
        before_resume: F,
    ) -> std::result::Result<(), AnswerFailure>
    where
        F: FnOnce(&str),
    {
        let mut all_pending = self.pending.lock().unwrap();
        let pending = all_pending
            .remove(question_id)
            .ok_or(AnswerFailure::NotFound)?;
        let run_id = pending.run_id;
        if pending.responder.is_closed() {
            return Err(AnswerFailure::Gone);
        }
        before_resume(&run_id);
        if pending.responder.send(QuestionResponse::Closed).is_err() {
            return Err(AnswerFailure::Gone);
        }
        self.remember(question_id, QuestionResolution::Closed);
        Ok(())
    }

    pub(crate) fn cancel_run(&self, run_id: &str) {
        let cancelled = {
            let mut pending = self.pending.lock().unwrap();
            let ids = pending
                .iter()
                .filter(|(_, question)| question.run_id == run_id)
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            ids.into_iter()
                .filter_map(|id| pending.remove(&id).map(|question| (id, question)))
                .collect::<Vec<_>>()
        };
        for (question_id, pending) in cancelled {
            let _ = pending.responder.send(QuestionResponse::Cancelled);
            self.remember(&question_id, QuestionResolution::Cancelled);
        }
    }
}

// ── normalize_answers ──
pub(crate) fn normalize_answers(
    request: &QuestionRequest,
    mut answers: QuestionAnswers,
) -> std::result::Result<QuestionAnswers, String> {
    for answer in &mut answers {
        for value in answer {
            *value = value.trim().to_string();
            if value.chars().any(char::is_control) {
                return Err("answers cannot contain control characters".to_string());
            }
        }
    }
    crate::question::validate_answers(request, &answers)
        .map_err(|error| safe_error_message(&error))?;
    Ok(answers)
}

// ── constant_time_eq ──
pub(crate) fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    let length = left.len().max(right.len());
    for index in 0..length {
        let left = left.get(index).copied().unwrap_or(0);
        let right = right.get(index).copied().unwrap_or(0);
        difference |= usize::from(left ^ right);
    }
    difference == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::question::{QuestionOption, QuestionPrompt};

    fn request() -> QuestionRequest {
        QuestionRequest {
            questions: vec![QuestionPrompt {
                header: "Pick".to_string(),
                question: "Pick one".to_string(),
                options: vec![QuestionOption {
                    label: "A".to_string(),
                    description: String::new(),
                }],
                multiple: false,
                custom: false,
            }],
        }
    }

    /// 句柄要留着：接收端一丢，`responder.is_closed()` 为真，answer/close 会
    /// 直接报 `Gone`（这正是 daemon 侧「前端已经走了」的判据）。
    fn insert_question(
        broker: &QuestionBroker,
        run_id: &str,
    ) -> (String, oneshot::Receiver<QuestionResponse>) {
        let (responder, receiver) = oneshot::channel();
        (broker.insert(run_id, request(), responder), receiver)
    }

    /// 面板轮询的依据：答完即从 `pending` 删，只有留档能证明「刚被答过」。
    #[test]
    fn state_reports_pending_then_answered() {
        let broker = QuestionBroker::new();
        let (question_id, _receiver) = insert_question(&broker, "run-1");
        assert_eq!(broker.state(&question_id).state_name(), "pending");

        broker
            .answer(&question_id, vec![vec!["A".to_string()]], |_, _| {})
            .unwrap();
        assert_eq!(broker.state(&question_id).state_name(), "answered");
        assert_eq!(
            broker.state(&question_id).answers(),
            Some(&vec![vec!["A".to_string()]])
        );
        assert_eq!(
            broker.state("question-nobody-asked").state_name(),
            "unknown"
        );
    }

    #[test]
    fn close_and_cancel_leave_a_trace() {
        let broker = QuestionBroker::new();
        let (closed, _closed_receiver) = insert_question(&broker, "run-1");
        broker.close(&closed, |_| {}).unwrap();
        assert_eq!(broker.state(&closed).state_name(), "closed");
        assert!(broker.state(&closed).answers().is_none());

        let (cancelled, _cancelled_receiver) = insert_question(&broker, "run-2");
        broker.cancel_run("run-2");
        assert_eq!(broker.state(&cancelled).state_name(), "cancelled");
    }

    /// 留档有界，不会给常驻 daemon 攒一张无界表。
    #[test]
    fn resolved_history_stays_bounded() {
        let broker = QuestionBroker::new();
        let mut receivers = Vec::new();
        for _ in 0..RESOLVED_HISTORY + 8 {
            let (question_id, receiver) = insert_question(&broker, "run-1");
            receivers.push(receiver);
            broker
                .answer(&question_id, vec![vec!["A".to_string()]], |_, _| {})
                .unwrap();
        }
        let resolved = broker.resolved.lock().unwrap();
        assert!(resolved.len() <= RESOLVED_HISTORY);
        drop(receivers);
    }
}
