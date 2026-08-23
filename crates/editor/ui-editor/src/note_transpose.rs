//! 音符移调操作模块

use super::Editor;
use lumino_editor_state::EditorTransform;

use std::collections::HashSet;

impl Editor {
    /// 将选中的音符移调指定半音数。
    ///
    /// # 参数
    /// * `semitones` — 半音数（可为负表示降调）
    ///
    /// # 返回
    /// 实际发生移动的音符数量。
    pub fn transpose_selected(&mut self, semitones: i16) -> usize {
        let selected: HashSet<usize> = self.get_selected_indices().into_iter().collect();
        let result = self.editor_state.data.transpose(&selected, semitones);
        if result > 0 {
            self.mark_notes_changed();
            // 2026-09 协作修复：移调前向须立即广播，否则队列堆积后被延迟 flush 会
            // 命中漂移位置，导致 B 端生成克隆体。
            self.broadcast_pending_collab_transform_sync();
        }
        result
    }
}
