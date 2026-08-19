//! 编辑器操作 - 音轨删除/找回/恢复
//!
//! 转发待删除音轨请求给 Runner（写入 `.lmdeltrack` 缓存）、
//! 找回删除音轨对话框、恢复音轨与永久销毁后的 ID 释放。

use std::time::{SystemTime, UNIX_EPOCH};

use crate::root::Root;
use lumino_message::events::window::track::{TrackDeletionNote, TrackDeletionPayload};
use lumino_midi_loader::NoteEvent;

impl Root {
    /// 消费 sidebar 中待删除音轨请求，构造 payload 并通过事件通道转发给 Runner
    ///
    /// 由 `handle_sidebar_event` 在 sidebar.update 后调用。pending_track_deletion
    /// 仅携带 track_id——元数据（名称/port/channel）从 `sidebar.tracks` 中查询
    /// （此时 tracks 中该音轨已 remove，故需在 remove 前缓存元数据），
    /// 音符列表从 `EditorData.document` 查询（单一权威源）。
    pub(crate) fn forward_pending_track_deletion(&mut self) {
        let track_id = match self.sidebar.take_pending_track_deletion() {
            Some(id) => id,
            None => return,
        };

        // 元数据缓存（由 sidebar 在删除入口时填充）
        let meta = match self.sidebar.take_pending_track_deletion_meta() {
            Some(m) => m,
            None => {
                tracing::warn!(
                    "forward_pending_track_deletion: 缺少元数据缓存 track_id={}，跳过缓存写入",
                    track_id
                );
                return;
            }
        };

        // 从 EditorData.document 提取音符（单一权威源）
        let notes = self.collect_track_notes_for_deletion(track_id);
        let note_count = notes.len() as u64;
        let max_tick = notes.iter().map(|n| n.end_tick).max().unwrap_or(0);

        // 通道 9（0-indexed）通常为鼓轨
        let is_drum = meta.channel == 9;

        let payload = TrackDeletionPayload {
            track_id: track_id as u16,
            track_name: meta.track_name,
            port: meta.port,
            channel: meta.channel,
            is_drum,
            max_tick,
            original_index: meta.original_index,
            notes,
        };

        tracing::info!(
            "Root: 转发音轨删除请求 track_id={} notes={} → Runner 写入 .lmdeltrack",
            track_id,
            note_count
        );

        lumino_message::events::emit(lumino_message::events::Event::Window(
            lumino_message::events::window::Event::delete_track(payload),
        ));
    }

    /// 消费 sidebar 中"找回删除音轨"对话框打开请求，转发给 Runner
    pub(crate) fn forward_pending_recover_track_dialog(&mut self) {
        if self.sidebar.take_pending_recover_track_dialog() {
            tracing::info!("Root: 请求打开找回删除音轨对话框");
            lumino_message::events::emit(lumino_message::events::Event::Window(
                lumino_message::events::window::Event::open_recover_track_dialog(),
            ));
        }
    }

    /// 从 EditorData.document 提取指定音轨的所有音符（用于删除缓存）
    fn collect_track_notes_for_deletion(&self, track_id: usize) -> Vec<TrackDeletionNote> {
        let data = &self.editor.editor_state.data;
        let source: Vec<NoteEvent> = data.track_notes(track_id).to_vec();

        let mut notes: Vec<TrackDeletionNote> = source
            .into_iter()
            .map(|n| TrackDeletionNote {
                start_tick: n.start_tick,
                end_tick: n.end_tick,
                key: n.key,
                velocity: n.velocity,
                channel: n.channel,
                port: self
                    .sidebar
                    .tracks
                    .iter()
                    .find(|t| t.id == track_id)
                    .map(|t| t.port)
                    .unwrap_or(0),
            })
            .collect();
        // 按 start_tick 排序，便于恢复时直接使用
        notes.sort_by_key(|n| n.start_tick);
        notes
    }

    /// Runner 在扫描缓存目录后调用：把条目列表填充到对话框状态
    pub fn apply_recover_track_entries(
        &mut self,
        entries: Vec<lumino_message::events::window::track::RecoverTrackEntryPayload>,
    ) {
        let ui_entries: Vec<crate::state::root_state::RecoverTrackEntry> = entries
            .into_iter()
            .map(|e| crate::state::root_state::RecoverTrackEntry {
                path: e.path,
                filename: e.filename,
                track_id: e.track_id,
                track_name: e.track_name,
                port: e.port,
                channel: e.channel,
                note_count: e.note_count,
                deleted_at: e.deleted_at,
                original_index: e.original_index,
            })
            .collect();
        self.set_recover_track_dialog_entries(ui_entries);
    }

    /// Runner 加载 `.lmdeltrack` 后调用：把音轨重新加入 sidebar.tracks + editor_state
    ///
    /// `original_index` 为删除时记录的位置，恢复时优先放回此位置；
    /// 若索引越界则追加到末尾。`reserved_track_ids` 中对应 ID 释放，
    /// 因为音轨重新出现在 sidebar.tracks 中。
    pub fn apply_track_restored(
        &mut self,
        payload: lumino_message::events::window::track::TrackDeletionPayload,
    ) {
        let track_id = payload.track_id as usize;

        // 构造 sidebar::Track
        let label = crate::sidebar::Sidebar::track_label(payload.port, payload.channel);
        let new_track = crate::sidebar::Track {
            id: track_id,
            name: payload.track_name.clone(),
            port: payload.port,
            channel: payload.channel,
            display_label: label,
            is_conductor: false,
            can_delete: true,
            is_muted: false,
            is_soloed: false,
            color: None,
        };

        // 把音轨放回原位置（若索引越界则追加）。
        // Conductor 首位不变量：不允许插到 conductor 之前（original_index=0 时改为其后）
        let conductor_idx = self.sidebar.tracks.iter().position(|t| t.is_conductor);
        let mut insert_idx = payload.original_index.min(self.sidebar.tracks.len());
        if let Some(ci) = conductor_idx {
            insert_idx = insert_idx.max(ci + 1).min(self.sidebar.tracks.len());
        }
        self.sidebar.tracks.insert(insert_idx, new_track);

        // 2026-08 修复：恢复轨 id 可能大于当前 document 轨数，先扩轨再写入，
        // 否则 replace_track_notes 越界静默失败，恢复的音符丢失。
        self.editor.editor_state.data.ensure_track(track_id);

        // 恢复音符到 EditorData.document（整轨替换，单一权威源）
        let restored_notes: Vec<NoteEvent> = payload
            .notes
            .iter()
            .map(|n| NoteEvent::new(n.start_tick, n.end_tick, n.key, n.velocity, n.channel))
            .collect();
        self.editor
            .editor_state
            .data
            .replace_track_notes(track_id, restored_notes);
        // 精确记录受影响音轨（洋葱皮事件级增量）
        self.editor
            .editor_state
            .data
            .mark_track_notes_changed_for(Some(std::collections::HashSet::from([track_id])));

        // 释放 reserved_track_id（音轨重新出现，不再占用）
        self.sidebar.release_reserved_track_id(track_id);

        // 恢复音轨改变了 sidebar 顺序 → 同步视觉位置映射（走带交互依赖）
        self.sync_track_visual_order();

        // 如果当前选中音轨已不存在（被删除时 selected_track 可能改向其他音轨），
        // 切换到恢复的音轨
        if !self
            .sidebar
            .tracks
            .iter()
            .any(|t| t.id == self.sidebar.selected_track)
        {
            self.sidebar.selected_track = track_id;
        }

        tracing::info!(
            "Root: 已恢复音轨 track_id={} notes={} → 位置 {}",
            track_id,
            payload.notes.len(),
            insert_idx
        );
    }

    /// Runner 销毁 `.lmdeltrack` 后调用：释放 reserved_track_id
    pub fn apply_track_permanently_deleted(&mut self, track_id: u16) {
        let id = track_id as usize;
        self.sidebar.release_reserved_track_id(id);
        tracing::info!("Root: 已永久销毁音轨缓存 track_id={}，释放 reserved ID", id);
    }

    /// 生成当前时间的 ISO 8601 字符串（用于 deleted_at 字段）
    #[allow(dead_code)]
    fn now_iso8601() -> String {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        // 简化版 ISO 8601：Unix 秒数（足够排序与显示）
        format!("ts:{}", secs)
    }
}
