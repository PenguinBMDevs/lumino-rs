//! 编辑器操作 - 播放管理

use crate::root::Root;

impl Root {
    /// 更新播放管理器中的音符数据
    pub fn update_playback_notes(&mut self) {
        if let Some(manager) = &mut self.playback_manager {
            let notes: Vec<crate::playback::NoteEvent> = self
                .editor
                .notes
                .iter()
                .map(|note| crate::playback::NoteEvent {
                    tick: note.tick,
                    channel: 0,
                    key: note.key as u8,
                    velocity: 100,
                    length: note.length,
                })
                .collect();

            manager.set_notes(notes);
            tracing::debug!(
                "Root::update_playback_notes: updated {} notes in playback manager",
                self.editor.notes.len()
            );
        }
    }
}
