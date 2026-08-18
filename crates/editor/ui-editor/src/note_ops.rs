//! 音符操作子模块入口
//!
//! 按职责拆分以满足 ≤400 行约束：
//! - `selection`: 选中集合增删改（insert/remove/clear/assign）
//! - `hit_test`: 音符命中检测（hit_test_note + note_hit_type）
//! - `delete`: 音符删除（delete_note_by_index/delete_note_at/delete_selected_notes）
//! - `selection_box`: 选择框边界计算（get_selection_box_bounds + hit_test_selection_box）

mod delete;
mod hit_test;
mod selection;
mod selection_box;

use super::Editor;

impl Editor {
    pub fn is_note_selected(&self, index: usize) -> bool {
        let interaction = &self.editor_state.interaction;
        // `selection_bitset` 优先：O(1) 位测试，零内存分配
        if let Some(ref bs) = interaction.selection_bitset {
            return bs.get(index);
        }
        interaction.selected_notes.contains(&index)
    }

    pub fn selected_notes_count(&self) -> usize {
        let interaction = &self.editor_state.interaction;
        if let Some(ref bs) = interaction.selection_bitset {
            return bs.count_ones();
        }
        interaction.selected_notes.len()
    }

    /// 是否有任何选中音符（同时检查 `selection_bitset` 和 `selected_notes`）
    pub fn has_selection(&self) -> bool {
        let interaction = &self.editor_state.interaction;
        if interaction.selection_bitset.is_some() {
            return true;
        }
        !interaction.selected_notes.is_empty()
    }

    /// 获取选中索引列表（兼容 `selection_bitset` 和 `selected_notes`）
    pub fn get_selected_indices(&self) -> Vec<usize> {
        let interaction = &self.editor_state.interaction;
        if let Some(ref bs) = interaction.selection_bitset {
            let mut indices = Vec::with_capacity(bs.count_ones());
            bs.for_each_set(|i| indices.push(i));
            return indices;
        }
        interaction.selected_notes.iter().copied().collect()
    }

    pub fn clear_selection(&mut self) {
        self.selection_clear();
    }

    pub fn select_all_notes(&mut self) {
        // 2026-08 单一权威源：NoteStore 已删除，全量选择直接走 document 访问器。
        self.selection_assign(self.editor_state.data.select_all_notes());
    }
}
