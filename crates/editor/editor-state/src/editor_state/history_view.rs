//! 撤销历史只读视图（DRAW-035）
//!
//! 历史面板只消费本模块的纯数据；真正的跳转必须在 ui-editor 的 `Editor`
//! 包装层执行，以便形状 group / 协作广播与普通音符一起回放。

use lumino_note_core::history::OpKind;

use super::EditorData;

/// 历史时间线上的一步（1-based 游标：执行完该步后的状态）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryStep {
    /// 跳到该步后的历史游标（0 = 初始状态，不在 steps 中）
    pub index: usize,
    /// 该步的操作类型（面板据此映射文案）
    pub kind: OpKind,
}

/// 历史面板视图快照
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryView {
    /// 全部步（最老在上；先过去的步骤，再未来的步骤）
    pub steps: Vec<HistoryStep>,
    /// 当前已执行的步数（0..=steps.len()）
    pub current: usize,
}

impl HistoryView {
    /// 时间线总长度（不含初始状态）
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// 是否没有任何历史
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

impl EditorData {
    /// 构造历史面板视图；O(历史条目数)，不克隆音符。
    pub fn history_view(&self) -> HistoryView {
        let undo_len = self.history.undo_len();
        let redo_len = self.history.redo_len();
        let mut steps = Vec::with_capacity(undo_len + redo_len);

        for (offset, entry) in self.history.undo_entries().enumerate() {
            steps.push(HistoryStep {
                index: offset + 1,
                kind: entry.op_kind(),
            });
        }
        for (offset, entry) in self.history.redo_entries().enumerate() {
            steps.push(HistoryStep {
                index: undo_len + offset + 1,
                kind: entry.op_kind(),
            });
        }

        HistoryView {
            steps,
            current: undo_len,
        }
    }

    /// 低层跳转（只改文档历史，不处理形状 group / 协作广播）。
    ///
    /// UI 必须使用 `lumino_ui_editor::Editor::history_jump_to`；本方法供
    /// editor-state 单测验证游标语义。
    pub fn history_jump_to(&mut self, target: usize) -> bool {
        let view = self.history_view();
        if target > view.len() {
            return false;
        }
        if target < view.current {
            for _ in target..view.current {
                if !self.undo() {
                    return false;
                }
            }
        } else {
            for _ in view.current..target {
                if !self.redo() {
                    return false;
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor_state::editor_data::EditorData;
    use lumino_note_core::note::Note;

    fn data_with_two_steps() -> EditorData {
        let mut data = EditorData::with_f32_notes(1, &[]);
        data.push_history_with_op_kind(OpKind::NoteCreate);
        data.insert_note(data.current_track, Note::new(0.0, 60, 1.0));
        data.push_history_with_op_kind(OpKind::NoteMove);
        data.insert_note(data.current_track, Note::new(2.0, 62, 1.0));
        data
    }

    #[test]
    fn test_history_view_orders_undo_then_redo() {
        let mut data = data_with_two_steps();
        assert_eq!(data.history_view().current, 2);

        data.undo();
        let view = data.history_view();
        assert_eq!(view.current, 1);
        assert_eq!(view.steps.len(), 2);
        assert_eq!(view.steps[0].index, 1);
        assert_eq!(view.steps[0].kind, OpKind::NoteCreate);
        assert_eq!(view.steps[1].index, 2);
        assert_eq!(view.steps[1].kind, OpKind::NoteMove);
    }

    #[test]
    fn test_history_jump_to_matches_repeated_undo_redo() {
        let mut data = data_with_two_steps();
        assert!(data.history_jump_to(0), "应能跳到初始状态");
        assert_eq!(data.current_track_note_count(), 0);
        assert_eq!(data.history_view().current, 0);

        assert!(data.history_jump_to(2), "应能跳到最新状态");
        assert_eq!(data.current_track_note_count(), 2);
        assert!(data.history_jump_to(1), "应能跳到中间状态");
        assert_eq!(data.current_track_note_count(), 1);
        assert!(!data.history_jump_to(99), "越界目标必须拒绝");
    }
}
