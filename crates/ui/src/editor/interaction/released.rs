use super::super::EditState;
use super::super::Note;
use crate::constants::editor::{DEFAULT_MIDI_CHANNEL, DEFAULT_NOTE_VELOCITY};
use lumino_core::event;

impl super::super::Editor {
    pub(crate) fn handle_released(&mut self) {
        match self.edit_state {
            EditState::Selecting { .. } => {
                if self.current_tool == crate::toolbar::Tool::Eraser {
                    self.delete_selected_notes();
                } else {
                    tracing::debug!("框选结束，选中 {} 个音符", self.selected_notes.len());
                }
            }
            EditState::Drawing {
                start_tick,
                key,
                current_tick,
            } => {
                self.finish_drawing(start_tick, key, current_tick);
            }
            EditState::PendingDrag { .. } => {}
            EditState::Dragging {
                note_index,
                original_tick,
                original_key,
                ..
            } => {
                self.finalize_dragging(note_index, original_tick, original_key);
            }
            EditState::ResizingStart { .. } | EditState::ResizingEnd { .. } => {
                tracing::debug!("Editor: 音符调整大小完成");
            }
            _ => {}
        }
        self.edit_state = EditState::Idle;
    }

    pub(crate) fn finish_drawing(&mut self, start_tick: f32, key: u16, current_tick: f32) {
        let (tick, length) = if current_tick > start_tick {
            (start_tick, current_tick - start_tick)
        } else if current_tick < start_tick {
            (current_tick, start_tick - current_tick)
        } else {
            (start_tick, self.state.default_note_length)
        };
        let length = length.max(self.state.snap_precision);

        self.push_history();
        let note = Note::new(tick, key, length);
        self.notes.push_back(note.clone());
        self.track_notes
            .insert(self.current_track, self.notes.clone());

        self.emit_note_added_event(&note);
        tracing::debug!(
            "编辑器: 已保存 {} 个音符到音轨 {}",
            self.notes.len(),
            self.current_track
        );
        self.mark_notes_changed();
    }

    fn emit_note_added_event(&self, note: &Note) {
        event::emit(event::Event::Window(event::window::Event::LocalNoteAdded {
            tick: note.tick,
            key: note.key,
            length: note.length,
            velocity: DEFAULT_NOTE_VELOCITY,
            channel: DEFAULT_MIDI_CHANNEL,
            track_index: self.current_track,
        }));
    }
}
