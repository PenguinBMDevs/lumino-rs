//! 批量删除音符操作（降级兼容层）
//!
//! NoteStore O(N) 热路径已删除，统一走 document 当前轨删除。
//! 保留签名兼容下游调用。

use std::collections::HashSet;

use super::super::EditorData;
use lumino_note_core::note_store::BitSet;

impl EditorData {
    /// 批量删除选中音符
    ///
    /// 返回删除的音符数。调用方需在调用前 `push_history()`。
    pub fn batch_delete_notes(&mut self, selected: &BitSet) -> usize {
        if selected.count_ones() == 0 {
            return 0;
        }

        let indices: HashSet<usize> = (0..self.current_track_note_count())
            .filter(|&idx| selected.get(idx))
            .collect();
        let before = self.current_track_note_count();
        self.delete_selected_notes(&indices);
        before - self.current_track_note_count()
    }

    /// 从 HashSet 批量删除选中音符（集成层适配）
    ///
    /// 把 `HashSet<usize>` 转为 `BitSet`，然后走 `batch_delete_notes`。
    /// 返回删除的音符数。调用方需在调用前 `push_history()`。
    pub fn batch_delete_notes_from_set(&mut self, selected: &HashSet<usize>) -> usize {
        if selected.is_empty() {
            return 0;
        }
        let bitset = BitSet::from_iter(self.current_track_note_count(), selected.iter().copied());
        self.batch_delete_notes(&bitset)
    }
}
