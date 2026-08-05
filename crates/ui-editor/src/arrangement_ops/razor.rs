//! 工程走带音符切割操作（Razor 工具）
//!
//! 在指定 tick/音轨处将音符一分为二。

use super::Editor;
use crate::note::Note;

impl Editor {
    /// 在指定 tick/音轨处分割音符（Razor 工具）。
    ///
    /// 返回实际分割的音符数。
    pub fn arrange_razor(&mut self, tick: f64, track: usize) -> usize {
        let tick_f = tick as f32;

        let indices_to_split = self.collect_razor_targets(tick_f, track);
        if indices_to_split.is_empty() {
            return 0;
        }

        self.push_history();

        let current_track = self.editor_state.data.current_track;
        let current_track_touched = track == current_track;
        let split_count = self.apply_razor_split(track, tick_f, indices_to_split);

        if split_count == 0 {
            self.editor_state.data.discard_last_history();
            return 0;
        }

        self.sync_current_track_after_arrange_op(current_track_touched);
        // 精确记录受影响音轨（洋葱皮事件级增量）
        self.editor_state
            .data
            .mark_track_notes_changed_for(Some(std::collections::HashSet::from([track])));
        tracing::info!(
            "Arrangement: 分割 {} 个音符 (tick={}, track={})",
            split_count,
            tick,
            track
        );
        split_count
    }

    /// 收集需要切割的音符索引。
    fn collect_razor_targets(&self, tick_f: f32, track: usize) -> Vec<usize> {
        let editor_data = &self.editor_state.data;
        let Some(notes) = editor_data.track_notes.get(&track) else {
            return Vec::new();
        };
        notes
            .iter()
            .enumerate()
            .filter_map(|(i, note)| {
                if note.tick < tick_f && note.tick + note.length > tick_f {
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
        let editor_data = &mut self.editor_state.data;
        let Some(notes) = editor_data.track_notes.get_mut(&track) else {
            return 0;
        };
        let mut split_count = 0usize;
        // 从后往前分割，避免索引漂移
        for idx in indices_to_split.into_iter().rev() {
            if let Some(note) = notes.get(idx).cloned() {
                let left = Note::from_raw(
                    note.tick,
                    note.key,
                    tick_f - note.tick,
                    note.velocity,
                    note.channel,
                );
                let right = Note::from_raw(
                    tick_f,
                    note.key,
                    note.tick + note.length - tick_f,
                    note.velocity,
                    note.channel,
                );
                notes.remove(idx);
                notes.insert(idx, right);
                notes.insert(idx, left);
                split_count += 1;
            }
        }
        split_count
    }
}
