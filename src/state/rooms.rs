//! 多方聊天室：按显式房间 id 读写（房间驱动跑在后台，不读 `self.session()`）。

use crate::state::*;

impl StateStore {
    pub fn insert_room_participants(
        &self,
        room_id: &str,
        participants: &[RoomParticipant],
    ) -> Result<()> {
        self.conv_db.insert_room_participants(room_id, participants)
    }

    pub fn room_participants(&self, room_id: &str) -> Result<Vec<RoomParticipant>> {
        self.conv_db.room_participants(room_id)
    }

    pub fn is_room(&self, session_id: &str) -> Result<bool> {
        Ok(!self.conv_db.room_participants(session_id)?.is_empty())
    }

    pub fn room_ids(&self) -> Result<HashSet<String>> {
        self.conv_db.room_ids()
    }

    pub fn append_room_message(
        &self,
        room_id: &str,
        speaker_kind: &str,
        participant_id: &str,
        content: &str,
        run_id: &str,
    ) -> Result<RoomMessage> {
        self.conv_db
            .append_room_message(room_id, speaker_kind, participant_id, content, run_id)
    }

    pub fn room_messages_after(&self, room_id: &str, after: i64) -> Result<Vec<RoomMessage>> {
        self.conv_db.room_messages_after(room_id, after)
    }

    pub fn room_messages_tail(&self, room_id: &str, limit: usize) -> Result<Vec<RoomMessage>> {
        self.conv_db.room_messages_tail(room_id, limit)
    }

    pub fn set_room_watermark(
        &self,
        room_id: &str,
        participant_id: &str,
        watermark: i64,
    ) -> Result<()> {
        self.conv_db
            .set_room_watermark(room_id, participant_id, watermark)
    }
}
