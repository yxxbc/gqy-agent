//! 聊后复盘的落盘（v37 `session_reviews`）。
//!
//! 追加型子表（AGENTS §3.2）：每次复盘插一行，读取只看最新一行。空 notes
//! 也插——「复盘过、没问题」要能撤掉上一版提示。

use crate::state::conversation_db::*;

impl ConversationDb {
    pub fn insert_session_review(
        &self,
        session_id: &str,
        last_turn_id: &str,
        notes: &[String],
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO session_reviews (session_id, last_turn_id, notes_json, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                session_id,
                last_turn_id,
                serde_json::to_string(notes)?,
                chrono::Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// 该会话最新一次复盘的 (last_turn_id, notes)。从没复盘过返回 None。
    pub fn latest_session_review(&self, session_id: &str) -> Result<Option<(String, Vec<String>)>> {
        let conn = self.conn.lock().unwrap();
        let row = conn
            .query_row(
                "SELECT last_turn_id, notes_json FROM session_reviews
                 WHERE session_id = ?1 ORDER BY review_id DESC LIMIT 1",
                params![session_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        Ok(match row {
            Some((turn_id, json)) => {
                Some((turn_id, serde_json::from_str(&json).unwrap_or_default()))
            }
            None => None,
        })
    }
}

/// WebUI 记忆页「复盘」栏的一行。`current`：它是所在会话最新的一次复盘，
/// 也就是下一轮会进 system 侧的那一版（notes 为空则什么都不进）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionReviewRow {
    pub review_id: i64,
    pub session_id: String,
    pub session_name: String,
    pub created_at: String,
    pub notes: Vec<String>,
    pub current: bool,
}

impl ConversationDb {
    /// 某人格下所有会话的复盘，新→旧分页。
    pub fn list_session_reviews(
        &self,
        persona: &str,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<SessionReviewRow>, i64)> {
        let conn = self.conn.lock().unwrap();
        let total = conn.query_row(
            "SELECT COUNT(*) FROM session_reviews r
             JOIN sessions s ON s.session_id = r.session_id
             WHERE s.persona = ?1",
            params![persona],
            |row| row.get::<_, i64>(0),
        )?;
        let mut stmt = conn.prepare(
            "SELECT r.review_id, r.session_id, s.name, r.created_at, r.notes_json,
                    r.review_id = (SELECT MAX(review_id) FROM session_reviews
                                   WHERE session_id = r.session_id)
             FROM session_reviews r
             JOIN sessions s ON s.session_id = r.session_id
             WHERE s.persona = ?1
             ORDER BY r.review_id DESC LIMIT ?2 OFFSET ?3",
        )?;
        let rows = stmt
            .query_map(params![persona, limit as i64, offset as i64], |row| {
                Ok(SessionReviewRow {
                    review_id: row.get(0)?,
                    session_id: row.get(1)?,
                    session_name: row.get(2)?,
                    created_at: row.get(3)?,
                    notes: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                    current: row.get(5)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok((rows, total))
    }
}
