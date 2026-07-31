//! 工程走带音符擦除操作
//!
//! 删除矩形范围 (tick_start..tick_end, track_lo..=track_hi) 内的所有音符。

use super::helpers::note_in_rect;
use super::Editor;

impl Editor {
    /// 擦除工程走带矩形范围内的音符。
    ///
    /// 返回实际删除的音符数。
    pub fn arrange_erase(
        &mut self,
        tick_start: f64,
        tick_end: f64,
        track_lo: usize,
        track_hi: usize,
    ) -> usize {
        if tick_start >= tick_end || track_lo > track_hi {
            return 0;
        }

        let current_track = self.editor_state.data.current_track;
        let tracks_to_clean = self.collect_erase_targets(tick_start, tick_end, track_lo, track_hi);

        if tracks_to_clean.is_empty() {
            return 0;
        }

        self.push_history();

        let current_track_touched = tracks_to_clean.contains(&current_track);
        let deleted_count = self.apply_erase_internal(tick_start, tick_end, tracks_to_clean);

        if deleted_count == 0 {
            self.editor_state.data.discard_last_history();
            return 0;
        }

        self.sync_current_track_after_arrange_op(current_track_touched);
        self.editor_state.data.mark_track_notes_changed();
        tracing::info!(
            "Arrangement: 擦除 {} 个音符 (tick {}..{}, track {}..={})",
            deleted_count,
            tick_start,
            tick_end,
            track_lo,
            track_hi
        );
        deleted_count
    }

    /// 执行擦除：从目标音轨中删除匹配的音符。
    fn apply_erase_internal(
        &mut self,
        tick_start: f64,
        tick_end: f64,
        tracks_to_clean: Vec<usize>,
    ) -> usize {
        let mut deleted_count = 0usize;
        let editor_data = &mut self.editor_state.data;
        for track_idx in tracks_to_clean {
            if let Some(notes) = editor_data.track_notes.get_mut(&track_idx) {
                let before = notes.len();
                notes.retain(|note| !note_in_rect(note, tick_start, tick_end));
                deleted_count += before - notes.len();
            }
        }
        deleted_count
    }

    /// 收集擦除范围内包含音符的音轨列表。
    fn collect_erase_targets(
        &self,
        tick_start: f64,
        tick_end: f64,
        track_lo: usize,
        track_hi: usize,
    ) -> Vec<usize> {
        let editor_data = &self.editor_state.data;
        let mut tracks_to_clean: Vec<usize> = Vec::new();
        for track_idx in track_lo..=track_hi {
            if let Some(notes) = editor_data.track_notes.get(&track_idx) {
                let has_any = notes
                    .iter()
                    .any(|note| note_in_rect(note, tick_start, tick_end));
                if has_any {
                    tracks_to_clean.push(track_idx);
                }
            }
        }
        tracks_to_clean
    }
}
