use crate::message::EditorAction;

impl super::super::Editor {
    pub fn handle_action(&mut self, action: EditorAction) {
        self.pending_audio_actions.clear();

        match action {
            EditorAction::Pressed { pos, shift } => self.handle_pressed(pos, shift),
            EditorAction::Moved(pos) => self.handle_moved(pos),
            EditorAction::Released => self.handle_released(),
            EditorAction::Scrolled { delta_x, delta_y } => self.handle_scrolled(delta_x, delta_y),
            EditorAction::DoubleClicked(pos) => self.handle_double_clicked(pos),
            EditorAction::DeletePressed => self.handle_delete_pressed(),
            EditorAction::Cut => self.cut_selected_notes(),
            EditorAction::Copy => {
                self.copy_selected_notes();
            }
            EditorAction::Paste => self.paste_notes_from_clipboard(),
            EditorAction::SelectAll => self.select_all_notes(),
            EditorAction::Undo => {
                self.undo();
            }
            EditorAction::Redo => {
                self.redo();
            }
            EditorAction::Scrubbed { tick } => {
                self.playback_position = tick;
            }
        }
    }
}
