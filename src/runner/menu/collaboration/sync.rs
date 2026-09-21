//! Runner 协作：本地音符 / 音轨 / 选择同步发送

use super::{LocalNoteMove, LocalNoteSnapshot};
use crate::runner::RunnerInner;

impl RunnerInner {
    /// 处理本地笔记添加（同步到其他用户）
    pub(crate) fn handle_local_note_added(&self, note: LocalNoteSnapshot) {
        let LocalNoteSnapshot {
            id,
            tick,
            key,
            length,
            velocity,
            channel,
            track_index,
        } = note;
        if !self.collab_state.collaboration_service.is_connected() {
            return;
        }

        let location = NoteLocation {
            tick,
            key,
            track_index,
            channel,
        };
        let modifiers = NoteOperationModifiers {
            length,
            velocity,
            target_track: Some(track_index),
            tick_offset: None,
            key_offset: None,
        };
        let operation = build_sync_note_operation(
            lumino_collaboration::types::NoteAction::Add,
            id,
            &location,
            &modifiers,
        );

        if let Err(e) = self
            .collab_state
            .collaboration_service
            .send_note_batch(operation)
        {
            tracing::debug!("协作: 发送笔记添加失败: {}", e);
        } else {
            tracing::info!("协作: 已发送笔记添加 - tick={}, key={}", tick, key);
        }
    }

    /// 处理本地批量音符添加（100K 粘贴，分片发送避免单帧过大）
    pub(crate) fn handle_local_notes_added_batch(
        &self,
        notes: Vec<(u64, f32, u16, f32, u8, u8, usize)>,
    ) {
        if !self.collab_state.collaboration_service.is_connected() {
            return;
        }
        if notes.is_empty() {
            return;
        }
        // 分片：每 10K 一条 NoteBatchOperation，避免单条 JSON 过大（WebSocket 帧限制）
        const CHUNK: usize = 10_000;
        for chunk in notes.chunks(CHUNK) {
            let sync_notes: Vec<lumino_collaboration::types::SyncNote> = chunk
                .iter()
                .map(|(id, tick, key, length, velocity, channel, track_index)| {
                    lumino_collaboration::types::SyncNote {
                        id: *id,
                        tick: *tick,
                        key: *key,
                        length: *length,
                        velocity: *velocity,
                        channel: *channel,
                        track_index: *track_index,
                    }
                })
                .collect();
            let operation = lumino_collaboration::types::NoteBatchOperation {
                action: lumino_collaboration::types::NoteAction::Add,
                notes: sync_notes,
                source_track: chunk.first().map(|(_, _, _, _, _, _, t)| *t),
                target_track: chunk.first().map(|(_, _, _, _, _, _, t)| *t),
                tick_offset: None,
                key_offset: None,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
            };
            if let Err(e) = self
                .collab_state
                .collaboration_service
                .send_note_batch(operation)
            {
                tracing::debug!("协作: 发送批量添加失败: {}", e);
                break;
            }
        }
        tracing::info!("协作: 已发送批量添加 - 总数 {}", notes.len());
    }

    /// 处理本地音符移动（同步到其他用户）
    pub(crate) fn handle_local_note_moved(&self, mv: LocalNoteMove) {
        let LocalNoteMove {
            id,
            tick,
            key,
            length,
            tick_offset,
            key_offset,
            track_index,
        } = mv;
        if !self.collab_state.collaboration_service.is_connected() {
            return;
        }

        let location = NoteLocation {
            tick,
            key,
            track_index,
            channel: 0,
        };
        let modifiers = NoteOperationModifiers {
            length,
            velocity: 100,
            target_track: Some(track_index),
            tick_offset: Some(tick_offset),
            key_offset: Some(key_offset),
        };
        let operation = build_sync_note_operation(
            lumino_collaboration::types::NoteAction::Move,
            id,
            &location,
            &modifiers,
        );

        if let Err(e) = self
            .collab_state
            .collaboration_service
            .send_note_batch(operation)
        {
            tracing::debug!("协作: 发送音符移动失败: {}", e);
        } else {
            tracing::info!(
                "协作: 已发送音符移动 - tick={}, key={}, offset=({}, {})",
                tick,
                key,
                tick_offset,
                key_offset
            );
        }
    }

    /// 处理本地音符删除（同步到其他用户）
    pub(crate) fn handle_local_note_deleted(&self, note: LocalNoteSnapshot) {
        let LocalNoteSnapshot {
            id,
            tick,
            key,
            length,
            velocity,
            channel,
            track_index,
        } = note;
        if !self.collab_state.collaboration_service.is_connected() {
            return;
        }

        let location = NoteLocation {
            tick,
            key,
            track_index,
            channel,
        };
        let modifiers = NoteOperationModifiers {
            length,
            velocity,
            target_track: None,
            tick_offset: None,
            key_offset: None,
        };
        let operation = build_sync_note_operation(
            lumino_collaboration::types::NoteAction::Delete,
            id,
            &location,
            &modifiers,
        );

        if let Err(e) = self
            .collab_state
            .collaboration_service
            .send_note_batch(operation)
        {
            tracing::debug!("协作: 发送音符删除失败: {}", e);
        } else {
            tracing::info!("协作: 已发送音符删除 - tick={}, key={}", tick, key);
        }
    }

    /// 处理本地音轨添加（同步到其他用户）
    pub(crate) fn handle_local_track_added(&self, track_index: usize) {
        if !self.collab_state.collaboration_service.is_connected() {
            return;
        }

        let update = lumino_collaboration::types::ProjectUpdate {
            update_type: lumino_collaboration::types::ProjectUpdateType::Track,
            data: serde_json::json!({
                "action": "add",
                "trackIndex": track_index,
            }),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        };

        if let Err(e) = self
            .collab_state
            .collaboration_service
            .send_project_update(update)
        {
            tracing::debug!("协作: 发送音轨添加失败: {}", e);
        } else {
            tracing::info!("协作: 已发送音轨添加 - track_index={}", track_index);
        }
    }

    /// 处理本地选择变更（同步到其他用户）
    ///
    /// 构造 `{active, timestamp, fingerprints}` JSON 并经协作通道广播。
    pub(crate) fn handle_local_selection_changed(
        &self,
        active: bool,
        timestamp: u64,
        fingerprints: Vec<[f64; 4]>,
    ) {
        if !self.collab_state.collaboration_service.is_connected() {
            return;
        }

        let selection = serde_json::json!({
            "active": active,
            "timestamp": timestamp,
            "fingerprints": fingerprints,
        });

        if let Err(e) = self
            .collab_state
            .collaboration_service
            .send_selection(selection)
        {
            tracing::debug!("协作: 发送选择变更失败: {}", e);
        } else {
            tracing::debug!(
                "协作: 已发送选择变更 - active={}, timestamp={}, 指纹数={}",
                active,
                timestamp,
                fingerprints.len()
            );
        }
    }
}

// ── Helper functions ─────────────────────────────────────────────────

/// 音符定位信息。
///
/// 用于聚合标识一个音符在工程中的位置与所属音轨/通道。
#[derive(Debug, Clone, Copy)]
struct NoteLocation {
    /// 时间刻度（tick）
    tick: f32,
    /// 音高键位
    key: u16,
    /// 所属音轨索引
    track_index: usize,
    /// MIDI 通道
    channel: u8,
}

/// 音符操作修饰参数。
///
/// 用于聚合构建 `NoteBatchOperation` 时补充的时长、力度、目标音轨及偏移信息。
#[derive(Debug, Clone, Copy)]
struct NoteOperationModifiers {
    /// 音符长度
    length: f32,
    /// 音符力度
    velocity: u8,
    /// 目标音轨（移动操作时使用）
    target_track: Option<usize>,
    /// 时间偏移（移动操作时使用）
    tick_offset: Option<f32>,
    /// 键位偏移（移动操作时使用）
    key_offset: Option<i16>,
}

/// 根据操作类型、真实音符 ID 与修饰参数构建同步操作。
///
/// `note_id` 为发送端文档分配的全局唯一音符 ID（来自 `LocalNoteAdded/Moved/Deleted`
/// 事件透传），取代原先基于时间戳的伪 ID，使对端能按 id 精确匹配同一音符，
/// 并避免不同客户端分配器之间的 id 碰撞。
fn build_sync_note_operation(
    action: lumino_collaboration::types::NoteAction,
    note_id: u64,
    location: &NoteLocation,
    modifiers: &NoteOperationModifiers,
) -> lumino_collaboration::types::NoteBatchOperation {
    let note = lumino_collaboration::types::SyncNote {
        id: note_id,
        tick: location.tick,
        key: location.key,
        length: modifiers.length,
        velocity: modifiers.velocity,
        channel: location.channel,
        track_index: location.track_index,
    };

    lumino_collaboration::types::NoteBatchOperation {
        action,
        notes: vec![note],
        source_track: Some(location.track_index),
        target_track: modifiers.target_track,
        tick_offset: modifiers.tick_offset,
        key_offset: modifiers.key_offset,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
    }
}
