//! 事件列表音符编辑操作。
//!
//! 音符的删除、插入与字段修改（起始 tick / 结束 tick / gate / key / velocity）。
//! 与 `event_list.rs` 主文件共享 `Root`，按职责拆分保持文件 < 400 行。

use crate::root::Root;
use crate::sidebar::event_browser::NoteRef;
use lumino_note_core::note::Note;

impl Root {
    /// 删除选中的音符（按 tick 匹配当前音轨）。
    pub(super) fn apply_delete_selected(&mut self, ticks: std::collections::HashSet<u32>) {
        if ticks.is_empty() {
            return;
        }
        let data = &mut self.editor.editor_state.data;
        data.delete_notes_at_ticks(&ticks);
        // 清空选中状态
        self.sidebar.event_browser_state.selected_ticks.clear();
        self.sidebar.event_browser_state.last_clicked_tick = None;
    }

    /// 通过 NoteRef 定位音符并应用修改。
    ///
    /// NoteRef 的 `id` 是 (tick, key, length, velocity, channel, track) 的哈希，
    /// 通过匹配原始字段定位 `notes` 中的索引，避免修改后索引漂移。
    pub(super) fn apply_note_edit(&mut self, note_ref: &NoteRef, f: impl Fn(&mut Note)) {
        let target = (note_ref.start_tick as f32, note_ref.key as u16);
        let data = &mut self.editor.editor_state.data;
        let Some(idx) = data.notes.iter().position(|n| (n.tick, n.key) == target) else {
            tracing::warn!(
                "Root: 未找到音符 start_tick={} key={}",
                note_ref.start_tick,
                note_ref.key
            );
            return;
        };
        data.push_history();
        if let Some(note) = data.notes.get_mut(idx) {
            f(note);
        }
        data.mark_track_notes_changed();
    }
}
