//! 编辑器操作 - 播放管理

use crate::root::Root;

impl Root {
    /// 更新播放管理器中的音符数据
    pub fn update_playback_notes(&mut self) {
        if let Some(manager) = &mut self.playback_manager {
            let velocity_threshold = self.velocity_filter_threshold;

            let notes: Vec<crate::playback::NoteEvent> = self
                .editor
                .notes
                .iter()
                .filter(|note| note.velocity > velocity_threshold)
                .map(|note| crate::playback::NoteEvent {
                    tick: note.tick,
                    channel: 0,
                    key: note.key as u8,
                    velocity: note.velocity,
                    length: note.length,
                })
                .collect();

            let note_count = notes.len();
            manager.set_notes(notes);
            tracing::debug!(
                "Root::update_playback_notes: updated {} notes in playback manager (过滤阈值={})",
                note_count,
                velocity_threshold
            );
        }
    }
}
