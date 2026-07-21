//! 音符删除操作：delete_note_by_index / delete_note_at / delete_selected_notes
//!
//! `delete_selected_notes` 使用 `for_each_note_view` 单次 O(N) 遍历
//! 捕获待删除音符信息，替代逐个 `get(i)` 的 O(K·log N) 开销。
//! - NoteStore 启用时：SoA 数组顺序遍历，cache-friendly
//! - NoteStore 未启用时：通过 `From<&Note>` 零 clone 构造 NoteView

use iced_core::Point;

use super::Editor;

impl Editor {
    pub fn delete_note_by_index(&mut self, index: usize) {
        // Capture note info before deletion for sync event
        let note_info = self.editor_state.data.get_note_view(index).map(|n| {
            (
                n.tick,
                n.key,
                n.length,
                n.velocity,
                n.channel,
                self.editor_state.data.current_track,
            )
        });

        self.editor_state.data.delete_note_by_index(index);
        self.editor_state.interaction.hover_state = None;
        self.mark_notes_changed();

        // Emit sync event for deletion
        if let Some((tick, key, length, velocity, channel, track_idx)) = note_info {
            lumino_event::emit(lumino_event::Event::Window(
                lumino_event::window::Event::local_note_deleted(
                    tick, key, length, velocity, channel, track_idx,
                ),
            ));
        }
    }

    pub fn delete_note_at(&mut self, pos: Point) -> bool {
        if let Some((index, _)) = self.hit_test_note(pos) {
            self.delete_note_by_index(index);
            true
        } else {
            false
        }
    }

    pub fn delete_selected_notes(&mut self) {
        let indices = self.editor_state.interaction.selected_notes.clone();
        if indices.is_empty() {
            return;
        }

        // 单次 O(N) 遍历捕获待删除音符信息，替代逐个 get(i) O(K·log N)
        //
        // 使用 for_each_note_view：
        // - NoteStore 启用时：SoA 数组顺序遍历，cache-friendly，16M 音符比
        //   im::Vector B-tree 快 10x+
        // - NoteStore 未启用时：通过 `From<&Note>` 零 clone 构造 NoteView
        let current_track = self.editor_state.data.current_track;
        let mut deleted_notes: Vec<_> = Vec::with_capacity(indices.len());
        self.editor_state.data.for_each_note_view(|i, n| {
            if indices.contains(&i) {
                deleted_notes.push((
                    n.tick,
                    n.key,
                    n.length,
                    n.velocity,
                    n.channel,
                    current_track,
                ));
            }
        });

        self.editor_state.data.delete_selected_notes(&indices);
        self.selection_clear();
        self.editor_state.interaction.hover_state = None;
        self.mark_notes_changed();

        // Emit sync events for each deleted note
        for (tick, key, length, velocity, channel, track_idx) in deleted_notes {
            lumino_event::emit(lumino_event::Event::Window(
                lumino_event::window::Event::local_note_deleted(
                    tick, key, length, velocity, channel, track_idx,
                ),
            ));
        }
    }
}
