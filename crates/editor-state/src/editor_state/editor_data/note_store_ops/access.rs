//! NoteStore 状态查询与迭代访问器（降级兼容层）
//!
//! NoteStore 已删除，访问器全部降级为直接遍历 `notes`（im::Vector）。
//! 保留签名兼容下游（ui-editor / ui）调用。

use super::super::EditorData;

impl EditorData {
    /// 检查 NoteStore 是否启用（降级恒为 false）
    pub fn is_note_store_enabled(&self) -> bool {
        false
    }

    /// 计算所有音符的边界（顺序扫描 notes）
    pub fn compute_all_notes_bounds(&self) -> (f32, f32, u16, u16) {
        let mut min_t = f32::INFINITY;
        let mut max_te = f32::NEG_INFINITY;
        let mut max_k = u16::MIN;
        let mut min_k = u16::MAX;
        for note in self.notes.iter() {
            min_t = min_t.min(note.tick);
            max_te = max_te.max(note.tick + note.length);
            max_k = max_k.max(note.key);
            min_k = min_k.min(note.key);
        }
        (min_t, max_te, max_k, min_k)
    }

    /// 获取音符只读视图（从 notes 取出后零 clone 转 NoteView）
    pub fn get_note_view(&self, idx: usize) -> Option<lumino_note_core::note_store::NoteView> {
        self.notes.get(idx).map(Into::into)
    }

    /// 遍历所有音符的 NoteView
    pub fn for_each_note_view(
        &self,
        mut f: impl FnMut(usize, lumino_note_core::note_store::NoteView),
    ) {
        for (note_idx, note) in self.notes.iter().enumerate() {
            f(note_idx, note.into());
        }
    }

    /// NoteStore 内存占用（降级恒为 0）
    pub fn note_store_memory_mb(&self) -> f64 {
        0.0
    }
}
