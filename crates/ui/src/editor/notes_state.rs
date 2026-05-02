use crate::editor::Editor;
use crate::message::AudioAction;

impl Editor {
    /// 获取并清空待处理的音频动作
    pub fn take_audio_actions(&mut self) -> Vec<AudioAction> {
        let actions = std::mem::take(&mut self.pending_audio_actions);
        if !actions.is_empty() {
            tracing::debug!("Editor: 取出了 {} 个音频动作", actions.len());
        }
        actions
    }

    /// 检查音符数据是否已变化
    pub fn notes_changed(&self) -> bool {
        self.notes_changed
    }

    /// 清除音符变化标志
    pub fn clear_notes_changed(&mut self) {
        self.notes_changed = false;
    }

    /// 标记音符数据已变化
    pub fn mark_notes_changed(&mut self) {
        self.notes_changed = true;
        self.note_index_dirty.set(true);
        self.track_note_indices
            .borrow_mut()
            .remove(&self.current_track);
    }
}
