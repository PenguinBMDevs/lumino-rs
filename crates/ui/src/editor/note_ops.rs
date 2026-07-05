use super::{Editor, HitType, SelectionHitType};
use crate::constants::editor::NOTE_EDGE_THRESHOLD_PX;
use crate::event;
use iced_core::Point;

impl Editor {
    pub fn hit_test_note(&self, pos: Point) -> Option<(usize, HitType)> {
        self.editor_state
            .hit_test_note((pos.x, pos.y), NOTE_EDGE_THRESHOLD_PX)
    }

    pub fn delete_note_by_index(&mut self, index: usize) {
        // Capture note info before deletion for sync event
        let note_info = self.editor_state.data.notes.get(index).map(|n| {
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
            event::emit(event::Event::Window(
                event::window::Event::local_note_deleted(
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

    pub fn is_note_selected(&self, index: usize) -> bool {
        self.editor_state
            .interaction
            .selected_notes
            .contains(&index)
    }

    pub fn selected_notes_count(&self) -> usize {
        self.editor_state.interaction.selected_notes.len()
    }

    pub fn clear_selection(&mut self) {
        self.editor_state.interaction.selected_notes.clear();
    }

    pub fn delete_selected_notes(&mut self) {
        let indices = self.editor_state.interaction.selected_notes.clone();

        // Capture note info before deletion for sync events
        let deleted_notes: Vec<_> = indices
            .iter()
            .filter_map(|&i| {
                self.editor_state.data.notes.get(i).map(|n| {
                    (
                        n.tick,
                        n.key,
                        n.length,
                        n.velocity,
                        n.channel,
                        self.editor_state.data.current_track,
                    )
                })
            })
            .collect();

        self.editor_state.data.delete_selected_notes(&indices);
        self.editor_state.interaction.selected_notes.clear();
        self.editor_state.interaction.hover_state = None;
        self.mark_notes_changed();

        // Emit sync events for each deleted note
        for (tick, key, length, velocity, channel, track_idx) in deleted_notes {
            event::emit(event::Event::Window(
                event::window::Event::local_note_deleted(
                    tick, key, length, velocity, channel, track_idx,
                ),
            ));
        }
    }

    pub fn get_notes_in_selection_box(
        &self,
        start_tick: f32,
        start_key: u16,
        current_tick: f32,
        current_key: u16,
    ) -> Vec<usize> {
        self.editor_state.get_notes_in_selection_box(
            start_tick,
            start_key,
            current_tick,
            current_key,
        )
    }

    pub(super) fn delete_notes_in_selection_box(
        &mut self,
        start_tick: f32,
        start_key: u16,
        current_tick: f32,
        current_key: u16,
    ) {
        let indices = self.editor_state.get_notes_in_selection_box(
            start_tick,
            start_key,
            current_tick,
            current_key,
        );
        if indices.is_empty() {
            return;
        }

        // Capture note info before deletion for sync events
        let deleted_notes: Vec<_> = indices
            .iter()
            .filter_map(|&i| {
                self.editor_state.data.notes.get(i).map(|n| {
                    (
                        n.tick,
                        n.key,
                        n.length,
                        n.velocity,
                        n.channel,
                        self.editor_state.data.current_track,
                    )
                })
            })
            .collect();

        let set: std::collections::HashSet<usize> = indices.into_iter().collect();
        self.editor_state.data.delete_selected_notes(&set);
        self.editor_state.interaction.selected_notes.clear();
        self.editor_state.interaction.hover_state = None;
        self.mark_notes_changed();

        // Emit sync events for each deleted note
        for (tick, key, length, velocity, channel, track_idx) in deleted_notes {
            event::emit(event::Event::Window(
                event::window::Event::local_note_deleted(
                    tick, key, length, velocity, channel, track_idx,
                ),
            ));
        }
    }

    pub fn select_all_notes(&mut self) {
        self.editor_state.interaction.selected_notes = self.editor_state.data.select_all_notes();
    }

    pub fn get_selection_box_bounds(&self) -> Option<(f32, f32, f32, f32)> {
        self.editor_state.get_selection_box_bounds()
    }

    pub fn hit_test_selection_box(&self, pos: Point) -> Option<SelectionHitType> {
        self.editor_state.hit_test_selection_box((pos.x, pos.y))
    }
}
