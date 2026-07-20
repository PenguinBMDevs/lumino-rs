//! 音符分割、合并与连奏操作模块

use super::Editor;

impl Editor {
    pub fn split_note(&mut self, index: usize, split_tick: f32) -> bool {
        let result = self.editor_state.data.split_note(index, split_tick);
        if result {
            self.selection_clear();
            self.editor_state.interaction.hover_state = None;
            self.mark_notes_changed();
        }
        result
    }

    pub fn glue_selected_notes(&mut self) -> usize {
        let selected = self.editor_state.interaction.selected_notes.clone();
        let result = self.editor_state.data.glue_selected_notes(&selected);
        if result > 0 {
            self.selection_clear();
            self.editor_state.interaction.hover_state = None;
            self.mark_notes_changed();
        }
        result
    }

    /// 连奏选中音符：按 tick 排序，填充相邻音符之间的间隙。
    /// 仅在有间隙时延长（不会缩短重叠音符）。最后一个音符保持不变。
    pub fn tie_selected_notes(&mut self) -> usize {
        let selected = self.editor_state.interaction.selected_notes.clone();
        let result = self.editor_state.data.tie_selected_notes(&selected);
        if result > 0 {
            self.selection_clear();
            self.editor_state.interaction.hover_state = None;
            self.mark_notes_changed();
        }
        result
    }
}
