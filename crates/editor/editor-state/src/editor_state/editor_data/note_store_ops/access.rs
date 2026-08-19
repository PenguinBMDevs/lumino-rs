//! NoteStore 状态查询与迭代访问器（降级兼容层）
//!
//! NoteStore 已删除，访问器直接遍历 document 当前轨（NoteEvent → NoteView 转换）。
//! 保留签名兼容下游（ui-editor / ui）调用。

use super::super::EditorData;

impl EditorData {
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
}
