//! 多方聊天室的落盘（v38 `room_participants` / `room_messages`）。
//!
//! 房间是一个普通 user 会话；这里只存「谁在房间里」和「房间里说了什么」。
//! 发言记录只追加（AGENTS §3.2），按 `message_id` 自增排序，它就是房间里的
//! 时间线。每位参与者的 `watermark` 记它最后看过的那条消息，下一次轮到它时
//! 只把水位之后的新消息打包给它（与 QQ 群聊的水位线同一思路）。

use crate::state::conversation_db::*;

/// 发言人类别。`notice` 是宿主写进房间的提示（某位参与者没回上来之类），
/// 只给人看，不进任何参与者的上下文。
pub const ROOM_SPEAKER_USER: &str = "user";
pub const ROOM_SPEAKER_PARTICIPANT: &str = "participant";
pub const ROOM_SPEAKER_NOTICE: &str = "notice";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomParticipant {
    pub participant_id: String,
    pub label: String,
    /// `persona`：顾清影本人（按人格提示词说话）；`relay` / `model`：以自身
    /// 名义出场的模型。
    pub kind: String,
    pub provider_id: String,
    pub model: String,
    pub backing_session_id: String,
    pub sort: i64,
    pub watermark: i64,
    /// 这位参与者的发言要不要进长期记忆。
    pub memory: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RoomMessage {
    pub message_id: i64,
    pub speaker_kind: String,
    pub participant_id: String,
    pub content: String,
    pub run_id: String,
    pub created_at: String,
}

fn room_message_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RoomMessage> {
    Ok(RoomMessage {
        message_id: row.get(0)?,
        speaker_kind: row.get(1)?,
        participant_id: row.get(2)?,
        content: row.get(3)?,
        run_id: row.get(4)?,
        created_at: row.get(5)?,
    })
}

const ROOM_MESSAGE_COLUMNS: &str =
    "message_id, speaker_kind, participant_id, content, run_id, created_at";

impl ConversationDb {
    pub fn insert_room_participants(
        &self,
        room_id: &str,
        participants: &[RoomParticipant],
    ) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        for participant in participants {
            tx.execute(
                "INSERT INTO room_participants (
                     room_id, participant_id, label, kind, provider_id, model,
                     backing_session_id, sort, watermark, memory
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    room_id,
                    participant.participant_id,
                    participant.label,
                    participant.kind,
                    participant.provider_id,
                    participant.model,
                    participant.backing_session_id,
                    participant.sort,
                    participant.watermark,
                    participant.memory,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// 按发言顺序排好的参与者。不是房间时返回空表。
    pub fn room_participants(&self, room_id: &str) -> Result<Vec<RoomParticipant>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT participant_id, label, kind, provider_id, model, backing_session_id,
                    sort, watermark, memory
             FROM room_participants WHERE room_id = ?1 ORDER BY sort, participant_id",
        )?;
        let rows = stmt
            .query_map(params![room_id], |row| {
                Ok(RoomParticipant {
                    participant_id: row.get(0)?,
                    label: row.get(1)?,
                    kind: row.get(2)?,
                    provider_id: row.get(3)?,
                    model: row.get(4)?,
                    backing_session_id: row.get(5)?,
                    sort: row.get(6)?,
                    watermark: row.get(7)?,
                    memory: row.get(8)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// 所有房间的会话 id（会话列表据此给房间打标记）。房间数量是人手建的，
    /// 一次全取比逐个查便宜。
    pub fn room_ids(&self) -> Result<std::collections::HashSet<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT DISTINCT room_id FROM room_participants")?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<_, _>>()?;
        Ok(rows)
    }

    pub fn append_room_message(
        &self,
        room_id: &str,
        speaker_kind: &str,
        participant_id: &str,
        content: &str,
        run_id: &str,
    ) -> Result<RoomMessage> {
        let conn = self.conn.lock().unwrap();
        let created_at = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO room_messages (room_id, speaker_kind, participant_id, content, run_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![room_id, speaker_kind, participant_id, content, run_id, created_at],
        )?;
        Ok(RoomMessage {
            message_id: conn.last_insert_rowid(),
            speaker_kind: speaker_kind.to_string(),
            participant_id: participant_id.to_string(),
            content: content.to_string(),
            run_id: run_id.to_string(),
            created_at,
        })
    }

    /// `after` 之后的消息，旧→新。
    pub fn room_messages_after(&self, room_id: &str, after: i64) -> Result<Vec<RoomMessage>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&format!(
            "SELECT {ROOM_MESSAGE_COLUMNS} FROM room_messages
             WHERE room_id = ?1 AND message_id > ?2 ORDER BY message_id"
        ))?;
        let rows = stmt
            .query_map(params![room_id, after], room_message_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// 最近 `limit` 条消息，旧→新（打开房间时渲染用）。
    pub fn room_messages_tail(&self, room_id: &str, limit: usize) -> Result<Vec<RoomMessage>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&format!(
            "SELECT {ROOM_MESSAGE_COLUMNS} FROM (
                 SELECT {ROOM_MESSAGE_COLUMNS} FROM room_messages
                 WHERE room_id = ?1 ORDER BY message_id DESC LIMIT ?2
             ) ORDER BY message_id"
        ))?;
        let rows = stmt
            .query_map(params![room_id, limit as i64], room_message_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn set_room_watermark(
        &self,
        room_id: &str,
        participant_id: &str,
        watermark: i64,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE room_participants SET watermark = MAX(watermark, ?3)
             WHERE room_id = ?1 AND participant_id = ?2",
            params![room_id, participant_id, watermark],
        )?;
        Ok(())
    }
}
