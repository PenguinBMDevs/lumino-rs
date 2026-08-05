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
    /// 通过匹配原始字段定位音符索引，避免修改后索引漂移。
    /// 2026-08 单一权威源：从 document 读取（track_notes 缓存已删除）。
    pub(super) fn apply_note_edit(&mut self, note_ref: &NoteRef, f: impl Fn(&mut Note)) {
        let target = (note_ref.start_tick as f32, note_ref.key as u16);
        let data = &mut self.editor.editor_state.data;
        let track_idx = data.current_track;
        let Some(idx) = data
            .current_track_notes()
            .iter()
            .position(|n| (n.start_tick as f32, n.key as u16) == target)
        else {
            tracing::warn!(
                "Root: 未找到音符 start_tick={} key={}",
                note_ref.start_tick,
                note_ref.key
            );
            return;
        };
        data.push_history();
        // 复制原始音符（NoteEvent 为 Copy），应用修改闭包后写回 document
        // （update_note 保持升序不变式；先取值再写回避免借用冲突）
        let original = data.current_track_notes()[idx];
        let mut note = Note::from_raw(
            original.start_tick as f32,
            original.key as u16,
            (original.end_tick - original.start_tick) as f32,
            original.velocity,
            original.channel,
        );
        f(&mut note);
        data.update_note(track_idx, idx, note);
        // 事件列表只编辑当前音轨 → 精确标记（洋葱皮豁免）
        data.mark_current_track_changed();
    }
}
