//! NoteStore 状态查询与迭代访问器（降级兼容层）
//!
//! NoteStore 已删除，访问器直接遍历 document 当前轨（NoteEvent → NoteView 转换）。
//! 保留签名兼容下游（ui-editor / ui）调用。

use super::super::EditorData;

impl EditorData {
    /// 检查 NoteStore 是否启用（降级恒为 false）
    pub fn is_note_store_enabled(&self) -> bool {
        false
    }

    /// 计算所有音符的边界（顺序扫描 document 当前轨）
    pub fn compute_all_notes_bounds(&self) -> (f32, f32, u16, u16) {
        let mut min_t = f32::INFINITY;
        let mut max_te = f32::NEG_INFINITY;
        let mut max_k = u16::MIN;
        let mut min_k = u16::MAX;
        for note in self.current_track_notes().iter() {
            let tick = note.start_tick as f32;
            let end = note.end_tick as f32;
            min_t = min_t.min(tick);
            max_te = max_te.max(end);
            max_k = max_k.max(note.key as u16);
            min_k = min_k.min(note.key as u16);
        }
        (min_t, max_te, max_k, min_k)
    }

    /// 获取音符只读视图（从 document 当前轨转换）
    pub fn get_note_view(&self, idx: usize) -> Option<lumino_note_core::note_store::NoteView> {
        self.current_track_notes()
            .get(idx)
            .map(|note| lumino_note_core::note_store::NoteView {
                tick: note.start_tick as f32,
                key: note.key as u16,
                length: (note.end_tick - note.start_tick) as f32,
                velocity: note.velocity,
                channel: note.channel,
            })
    }

    /// 遍历所有音符的 NoteView（从 document 当前轨转换）
    pub fn for_each_note_view(
        &self,
        mut f: impl FnMut(usize, lumino_note_core::note_store::NoteView),
    ) {
        for (note_idx, note) in self.current_track_notes().iter().enumerate() {
            f(
                note_idx,
                lumino_note_core::note_store::NoteView {
                    tick: note.start_tick as f32,
                    key: note.key as u16,
                    length: (note.end_tick - note.start_tick) as f32,
                    velocity: note.velocity,
                    channel: note.channel,
                },
            );
        }
    }

    /// NoteStore 内存占用（降级恒为 0）
    pub fn note_store_memory_mb(&self) -> f64 {
        0.0
    }
}
