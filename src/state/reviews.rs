//! 聊后复盘：按显式会话 id 读写。复盘跑在后台，期间 daemon 可能已切到别的
//! 会话，所以一律带着排期时抓下的 session id，不读 `self.session()`。

use crate::state::*;

impl StateStore {
    pub fn insert_session_review(
        &self,
        session_id: &str,
        last_turn_id: &str,
        notes: &[String],
    ) -> Result<()> {
        self.conv_db
            .insert_session_review(session_id, last_turn_id, notes)
    }

    pub fn latest_session_review(&self, session_id: &str) -> Result<Option<(String, Vec<String>)>> {
        self.conv_db.latest_session_review(session_id)
    }

    pub fn list_session_reviews(
        &self,
        persona: &str,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<SessionReviewRow>, i64)> {
        self.conv_db.list_session_reviews(persona, limit, offset)
    }

    /// 最近 `limit` 个可见回合（不含压缩摘要与隐藏回合），旧→新。
    pub fn recent_turns_of(&self, session_id: &str, limit: usize) -> Result<Vec<Turn>> {
        let mut turns: Vec<Turn> = self
            .conv_db
            .load_turns(session_id)?
            .into_iter()
            .filter(|turn| !turn.is_summary && !turn.hidden)
            .collect();
        let start = turns.len().saturating_sub(limit);
        Ok(turns.split_off(start))
    }
}
