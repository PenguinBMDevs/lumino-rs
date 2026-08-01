//! NoteStore 状态查询与迭代访问器

use super::super::EditorData;

impl EditorData {
    /// 检查 NoteStore 是否启用
    pub fn is_note_store_enabled(&self) -> bool {
        self.note_store_enabled
    }

    /// 计算所有音符的边界（单次顺序扫描 NoteStore，避免 16M 次二分查找）
    pub fn compute_all_notes_bounds(&self) -> (f32, f32, u16, u16) {
        self.note_store.compute_bounds()
    }

    /// 获取音符只读视图（NoteStore 启用时走零 clone 路径）
    ///
    /// NoteView 是 Copy 语义，零成本传递。
    /// NoteStore 未启用时，从 im::Vector 取出 &Note 后零 clone 转 NoteView。
    pub fn get_note_view(&self, idx: usize) -> Option<lumino_note_core::note_store::NoteView> {
        if self.note_store_enabled {
            self.note_store.get_ref(idx)
        } else {
            self.notes.get(idx).map(Into::into)
        }
    }

    /// 遍历所有音符的 NoteView（NoteStore 启用时零 clone）
    ///
    /// 用于 hot path 替代 `notes.iter().enumerate()`，避免每个音符一次 Note clone。
    pub fn for_each_note_view(
        &self,
        mut f: impl FnMut(usize, lumino_note_core::note_store::NoteView),
    ) {
        if self.note_store_enabled {
            self.note_store.for_each_ref(f);
        } else {
            for (note_idx, note) in self.notes.iter().enumerate() {
                f(note_idx, note.into());
            }
        }
    }

    /// NoteStore 内存占用（MB）
    pub fn note_store_memory_mb(&self) -> f64 {
        if self.note_store_enabled {
            self.note_store.memory_mb()
        } else {
            0.0
        }
    }
}
