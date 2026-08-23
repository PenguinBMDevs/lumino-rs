//! 音符翻转操作模块

use super::Editor;
use lumino_editor_state::EditorTransform;
use std::collections::HashSet;

use lumino_ui_core::toolbar_event::FlipHorizontalMode;

impl Editor {
    /// 垂直翻转选中的音符（围绕键盘中心镜像 key）。
    ///
    /// 翻转后若发生改动，会清空选中并清除悬停状态。
    ///
    /// # 返回
    /// 实际发生翻转的音符数量。
    pub fn flip_selected_notes_vertical(&mut self) -> usize {
        let selected: HashSet<usize> = self.get_selected_indices().into_iter().collect();
        let max_key_index = (self.editor_state.view.visible_key_count - 1) as f32;
        let result = self
            .editor_state
            .data
            .flip_vertical(&selected, max_key_index);
        if result > 0 {
            self.selection_clear();
            self.editor_state.interaction.hover_state = None;
            self.mark_notes_changed();
            // 2026-09 协作修复：垂直翻转前向立即广播（防队列堆积→克隆体）。
            self.broadcast_pending_collab_transform_sync();
        }
        result
    }

    /// 水平翻转选中的音符（围绕指定轴计算镜像）。
    ///
    /// 翻转后若发生改动，会清空选中并清除悬停状态。
    ///
    /// # 参数
    /// * `mode` — 水平翻转模式（居中/左侧/右侧）
    ///
    /// # 返回
    /// 实际发生翻转的音符数量。
    pub fn flip_selected_notes_horizontal(&mut self, mode: FlipHorizontalMode) -> usize {
        let selected: HashSet<usize> = self.get_selected_indices().into_iter().collect();
        let indices: Vec<usize> = selected.iter().copied().collect();
        if indices.is_empty() {
            return 0;
        }
        // 2026-08 单一权威源：经 get_note_view 读取（NoteView: tick f32/length f32）
        let data = &self.editor_state.data;
        let mut min_tick = f32::INFINITY;
        let mut max_tick_end = f32::NEG_INFINITY;
        for &i in &indices {
            if let Some(n) = data.get_note_view(i) {
                min_tick = min_tick.min(n.tick);
                max_tick_end = max_tick_end.max(n.tick + n.length);
            }
        }
        let axis_tick = match mode {
            FlipHorizontalMode::Center => (min_tick + max_tick_end) / 2.0,
            FlipHorizontalMode::Left => min_tick,
            FlipHorizontalMode::Right => max_tick_end,
        };
        let result = self.editor_state.data.flip_horizontal(&selected, axis_tick);
        if result > 0 {
            self.selection_clear();
            self.editor_state.interaction.hover_state = None;
            self.mark_notes_changed();
            // 2026-09 协作修复：水平翻转前向立即广播（防队列堆积→克隆体）。
            self.broadcast_pending_collab_transform_sync();
        }
        result
    }
}
