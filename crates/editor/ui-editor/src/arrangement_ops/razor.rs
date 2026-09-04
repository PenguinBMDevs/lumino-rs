//! 工程走带音符切割操作（Razor 工具）
//!
//! 在指定 tick/音轨处将音符一分为二。

use super::Editor;
use crate::note::Note;
use lumino_editor_state::CollabTransformSyncEntry;

impl Editor {
    /// 在指定 tick/音轨处分割音符（Razor 工具）。
    ///
    /// `track` 为视觉位置（侧边栏顺序），内部映射为文档音轨索引，
    /// 保证拖动排序后切割落在正确的音轨上。
    ///
    /// 返回实际分割的音符数。
    pub fn arrange_razor(&mut self, tick: f64, track: usize) -> usize {
        let tick_f = tick as f32;

        // 视觉位置 → 文档音轨索引（拖动排序后二者不一致）
        let doc_track = self.editor_state.data.document_track_at(track);

        let indices_to_split = self.collect_razor_targets(tick_f, doc_track);
        if indices_to_split.is_empty() {
            return 0;
        }

        self.push_history();

        let current_track = self.editor_state.data.current_track;
        let current_track_touched = doc_track == current_track;
        let split_count = self.apply_razor_split(doc_track, tick_f, indices_to_split);

        if split_count == 0 {
            self.editor_state.data.discard_last_history();
            return 0;
        }

        if current_track_touched {
            self.mark_notes_changed();
        }
        // 精确记录受影响音轨（洋葱皮事件级增量）
        self.editor_state
            .data
            .mark_track_notes_changed_for(Some(std::collections::HashSet::from([doc_track])));
        // 2026-09 协作修复：Razor 切割前向立即广播（删原+加左右），防 B 端失同步。
        self.broadcast_pending_collab_transform_sync();
        tracing::info!(
            "Arrangement: 分割 {} 个音符 (tick={}, visual={}, track={})",
            split_count,
            tick,
            track,
            doc_track
        );
        split_count
    }

    /// 收集需要切割的音符索引。
    fn collect_razor_targets(&self, tick_f: f32, track: usize) -> Vec<usize> {
        let editor_data = &self.editor_state.data;
        // 2026-08 单一权威源：从 document 读取（NoteEvent，u32 tick）
        editor_data
            .track_notes(track)
            .iter()
            .enumerate()
            .filter_map(|(i, note)| {
                let note_tick = note.start_tick as f32;
                let note_end = note.end_tick as f32;
                if note_tick < tick_f && note_end > tick_f {
                    Some(i)
                } else {
                    None
                }
            })
            .collect()
    }

    /// 执行切割：从后往前遍历索引，每个音符替换为两个新音符。
    fn apply_razor_split(
        &mut self,
        track: usize,
        tick_f: f32,
        indices_to_split: Vec<usize>,
    ) -> usize {
        let mut split_count = 0usize;
        // 2026-09 协作修复：切割改变音符数量，累积「删原 + 加左右」同步条目。
        // pending_collab_transform_sync 为 pub(crate)，ui-editor 不可直访，
        // 故经 pub 入口 push_collab_transform_entries 注入。
        let mut sync_entries: Vec<CollabTransformSyncEntry> = Vec::new();
        // 从后往前分割，避免索引漂移
        for idx in indices_to_split.into_iter().rev() {
            // 2026-08 单一权威源：从 document 删除原音符，再按序插入 left + right
            let Some(note) = self.editor_state.data.remove_note(track, idx) else {
                continue;
            };
            let note_tick = note.start_tick as f32;
            let note_key = note.key as u16;
            let note_length = (note.end_tick - note.start_tick) as f32;
            let left = Note::from_raw(
                note_tick,
                note_key,
                tick_f - note_tick,
                note.velocity,
                note.channel,
            );
            let right = Note::from_raw(
                tick_f,
                note_key,
                note_tick + note_length - tick_f,
                note.velocity,
                note.channel,
            );
            // insert_note 按 start_tick 有序插入，left/right 顺序由文档维护
            self.editor_state.data.insert_note(track, right);
            self.editor_state.data.insert_note(track, left);
            // id：删原用原音符真实 id；加左右经 note_id_at 反查刚插入音符的真实 id。
            let left_id = self
                .editor_state
                .data
                .note_id_at(track, note_tick, note_key)
                .unwrap_or(0);
            let right_id = self
                .editor_state
                .data
                .note_id_at(track, tick_f, note_key)
                .unwrap_or(0);
            sync_entries.push((
                false,
                note.id,
                note_tick,
                note_key,
                note_length,
                note.velocity,
                note.channel,
                track,
            ));
            sync_entries.push((
                true,
                left_id,
                note_tick,
                note_key,
                tick_f - note_tick,
                note.velocity,
                note.channel,
                track,
            ));
            sync_entries.push((
                true,
                right_id,
                tick_f,
                note_key,
                note_tick + note_length - tick_f,
                note.velocity,
                note.channel,
                track,
            ));
            split_count += 1;
        }
        self.editor_state
            .data
            .push_collab_transform_entries(sync_entries);
        split_count
    }
}
