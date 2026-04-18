use super::super::{EditState, HitType};
use crate::constants::editor::DEFAULT_NOTE_VELOCITY;
use crate::message::AudioAction;
use crate::toolbar::Tool;
use lumino_core::storage::config::EraserBehavior;

impl super::super::Editor {
    pub(crate) fn handle_pressed(&mut self, pos: iced_core::Point, shift: bool) {
        if !self.is_inside_canvas(pos) {
            return;
        }

        let tick = self.x_to_tick(pos.x);
        let key = self.y_to_key(pos.y);
        let snapped_tick = self.snap_tick(tick);

        self.handle_tool_pressed(pos, shift, snapped_tick, key);
    }

    pub(crate) fn handle_tool_pressed(
        &mut self,
        pos: iced_core::Point,
        shift: bool,
        snapped_tick: f32,
        key: u16,
    ) {
        let hit_result = self.hit_test_note(pos);

        match self.current_tool {
            Tool::Pointer => self.handle_pointer_pressed(pos, hit_result, snapped_tick),
            Tool::Pencil => self.handle_pencil_pressed(pos, hit_result, snapped_tick, key),
            Tool::Eraser => self.handle_eraser_pressed(pos, shift, hit_result),
            _ => self.handle_default_tool_pressed(pos, hit_result, snapped_tick, key),
        }
    }

    pub(crate) fn handle_pointer_pressed(
        &mut self,
        pos: iced_core::Point,
        hit_result: Option<(usize, HitType)>,
        snapped_tick: f32,
    ) {
        if let Some((index, hit_type)) = hit_result {
            if !self.selected_notes.contains(&index) {
                self.selected_notes.clear();
                self.selected_notes.insert(index);
            }
            self.start_note_edit(index, hit_type, pos);
        } else {
            self.playback_position = snapped_tick;
            self.selected_notes.clear();
            self.edit_state = EditState::Selecting {
                start_pos: pos,
                current_pos: pos,
            };
        }
    }

    pub(crate) fn handle_pencil_pressed(
        &mut self,
        pos: iced_core::Point,
        hit_result: Option<(usize, HitType)>,
        snapped_tick: f32,
        key: u16,
    ) {
        if let Some((index, hit_type)) = hit_result {
            self.start_note_edit(index, hit_type, pos);
        } else {
            self.start_drawing(snapped_tick, key);
        }
    }

    pub(crate) fn handle_eraser_pressed(
        &mut self,
        pos: iced_core::Point,
        shift: bool,
        hit_result: Option<(usize, HitType)>,
    ) {
        match self.state.eraser_behavior {
            EraserBehavior::Default => {
                if shift {
                    self.selected_notes.clear();
                    self.edit_state = EditState::Selecting {
                        start_pos: pos,
                        current_pos: pos,
                    };
                } else if hit_result.is_some() {
                    self.delete_note_at(pos);
                }
            }
            EraserBehavior::DirectSelect => {
                if shift && hit_result.is_some() {
                    self.delete_note_at(pos);
                } else {
                    self.selected_notes.clear();
                    self.edit_state = EditState::Selecting {
                        start_pos: pos,
                        current_pos: pos,
                    };
                }
            }
        }
    }

    pub(crate) fn handle_default_tool_pressed(
        &mut self,
        pos: iced_core::Point,
        hit_result: Option<(usize, HitType)>,
        snapped_tick: f32,
        key: u16,
    ) {
        if let Some((index, hit_type)) = hit_result {
            self.start_note_edit(index, hit_type, pos);
        } else {
            self.start_drawing(snapped_tick, key);
        }
    }

    fn start_note_edit(&mut self, index: usize, hit_type: HitType, pos: iced_core::Point) {
        match hit_type {
            HitType::Start => {
                self.push_history();
                let note = &self.notes[index];
                self.edit_state = EditState::ResizingStart {
                    note_index: index,
                    original_tick: note.tick,
                    original_length: note.length,
                };
            }
            HitType::End => {
                self.push_history();
                self.edit_state = EditState::ResizingEnd { note_index: index };
            }
            HitType::Middle => {
                let note = &self.notes[index];
                self.edit_state = EditState::PendingDrag {
                    note_index: index,
                    start_pos: pos,
                    original_tick: note.tick,
                    original_key: note.key,
                };
                self.play_note_audio(note.key, "点击音符");
            }
        }
    }

    fn start_drawing(&mut self, snapped_tick: f32, key: u16) {
        self.edit_state = EditState::Drawing {
            start_tick: snapped_tick,
            key,
            current_tick: snapped_tick,
        };
        self.play_note_audio(key, "新音符");
    }

    pub(crate) fn play_note_audio(&mut self, key: u16, _context: &str) {
        self.pending_audio_actions.push(AudioAction::PlayNote {
            key: key as u8,
            velocity: DEFAULT_NOTE_VELOCITY,
        });
    }
}
