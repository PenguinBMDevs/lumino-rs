//! 工程走带音符添加操作
//!
//! 在指定音轨和 tick 位置添加一个音符。

use super::Editor;
use crate::note::Note;

impl Editor {
    /// 在工程走带指定音轨 tick 处添加一个音符。
    ///
    /// 返回是否实际添加。
    pub fn arrange_add_note(
        &mut self,
        track_count: usize,
        track: usize,
        tick: f64,
        duration: f64,
        key: u8,
        velocity: u8,
    ) -> bool {
        if tick < 0.0 || duration <= 0.0 || track >= track_count {
            return false;
        }

        let note = Note::from_raw(tick as f32, key as u16, duration as f32, velocity, 0);

        self.push_history();

        let current_track = self.editor_state.data.current_track;
        let current_track_touched = track == current_track;

        // 2026-08 单一权威源：直接插入 document（按 start_tick 有序插入）
        {
            let editor_data = &mut self.editor_state.data;
            editor_data.insert_note(track, note);
        }

        if current_track_touched {
            self.mark_notes_changed();
        }
        // 精确记录受影响音轨（洋葱皮事件级增量）
        self.editor_state
            .data
            .mark_track_notes_changed_for(Some(std::collections::HashSet::from([track])));
        tracing::info!(
            "Arrangement: 添加音符 (tick={}, duration={}, track={}, key={}, velocity={})",
            tick,
            duration,
            track,
            key,
            velocity
        );
        true
    }
}
