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
        self.editor_state
            .interaction
            .selected_notes
            .contains(&index)
    }

    pub fn selected_notes_count(&self) -> usize {
        self.editor_state.interaction.selected_notes.len()
    }

    pub fn clear_selection(&mut self) {
        self.selection_clear();
    }

    pub fn select_all_notes(&mut self) {
        self.selection_assign(self.editor_state.data.select_all_notes());
    }
}
