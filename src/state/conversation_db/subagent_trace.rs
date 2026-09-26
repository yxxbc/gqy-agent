//! 子代理完整过程的落盘（v38 `subagent_trace`）。
//!
//! 追加型子表（AGENTS §3.2）：审计会话每一步追加一行标记，读取按 seq 顺序全取。
//! 思考/正文的逐 token 增量在写入前已由 `tools::subagent_trace` 合并成段。

use crate::state::conversation_db::*;

/// 审计会话的详情：会话行、那一个回合（prompt → 结果）、端点与用量。
#[derive(Debug, Clone)]
pub struct SubagentAuditDetail {
    pub record: SessionRecord,
    pub prompt: String,
    pub output: String,
    pub status: TurnStatus,
    pub provider_id: Option<String>,
    pub model: Option<String>,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub cache_read_tokens: i64,
}

impl ConversationDb {
    pub fn append_subagent_trace(&self, session_id: &str, markers: &[String]) -> Result<()> {
        if markers.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        {
            let mut stmt =
                tx.prepare("INSERT INTO subagent_trace (session_id, marker) VALUES (?1, ?2)")?;
            for marker in markers {
                stmt.execute(params![session_id, marker])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn subagent_trace(&self, session_id: &str) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT marker FROM subagent_trace WHERE session_id = ?1 ORDER BY seq ASC")?;
        let rows = stmt.query_map(params![session_id], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// 只认 `kind='subagent'` 的会话；别的会话一律当不存在。
    pub fn subagent_audit_detail(&self, session_id: &str) -> Result<Option<SubagentAuditDetail>> {
        let Some(record) = self.session_record(session_id)? else {
            return Ok(None);
        };
        if record.kind != "subagent" {
            return Ok(None);
        }
        let usage = {
            let conn = self.conn.lock().unwrap();
            conn.query_row(
                "SELECT provider_id, model, COALESCE(prompt_tokens, 0),
                        COALESCE(completion_tokens, 0), COALESCE(total_tokens, 0),
                        COALESCE(cache_read_tokens, 0)
                   FROM sessions WHERE session_id = ?1",
                params![session_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                    ))
                },
            )?
        };
        let turn = self.load_turns(session_id)?.into_iter().next();
        let (prompt, output, status) = match turn {
            Some(turn) => (turn.user_content, turn.assistant_content, turn.status),
            None => (String::new(), String::new(), TurnStatus::Running),
        };
        Ok(Some(SubagentAuditDetail {
            record,
            prompt,
            output,
            status,
            provider_id: usage.0,
            model: usage.1,
            prompt_tokens: usage.2,
            completion_tokens: usage.3,
            total_tokens: usage.4,
            cache_read_tokens: usage.5,
        }))
    }
}
