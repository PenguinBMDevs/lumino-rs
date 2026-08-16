//! 工程走带音符添加操作
//!
//! 在指定音轨和 tick 位置添加一个音符。

use super::Editor;
use crate::note::Note;

impl Editor {
    /// 在工程走带指定音轨 tick 处添加一个音符。
    ///
    /// `track` 为视觉位置（侧边栏顺序），内部映射为文档音轨索引后写入，
    /// 保证拖动排序后点击添加落在正确的音轨上。
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

        // 2026-08 单一权威源：直接插入 document（按 start_tick 有序插入）
        // 视觉位置 → 文档音轨索引（拖动排序后二者不一致）
        let doc_track = self.editor_state.data.document_track_at(track);
        {
            let editor_data = &mut self.editor_state.data;
            editor_data.insert_note(doc_track, note);
        }

        // 2026-08-06 音频修复：无论编辑哪个音轨都需要触发播放同步，
        // update_playback_notes 会发送完整 document 快照到播放引擎。
        self.mark_notes_changed();
        // 精确记录受影响音轨（洋葱皮事件级增量）
        self.editor_state
            .data
            .mark_track_notes_changed_for(Some(std::collections::HashSet::from([doc_track])));
        tracing::info!(
            "Arrangement: 添加音符 (tick={}, duration={}, visual={}, track={}, key={}, velocity={})",
            tick,
            duration,
            track,
            doc_track,
            key,
            velocity
        );
        true
    }
}
