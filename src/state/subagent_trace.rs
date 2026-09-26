//! 子代理完整过程：按审计会话 id 读写。子代理可能跑在后台，一律带着显式的
//! 会话 id，不读 `self.session()`。

use crate::state::*;

impl StateStore {
    pub fn append_subagent_trace(&self, session_id: &str, markers: &[String]) -> Result<()> {
        self.conv_db.append_subagent_trace(session_id, markers)
    }

    pub fn subagent_trace(&self, session_id: &str) -> Result<Vec<String>> {
        self.conv_db.subagent_trace(session_id)
    }

    pub fn subagent_audit_detail(&self, session_id: &str) -> Result<Option<SubagentAuditDetail>> {
        self.conv_db.subagent_audit_detail(session_id)
    }
}
