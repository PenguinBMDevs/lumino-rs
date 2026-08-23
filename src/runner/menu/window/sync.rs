//! 本地同步类窗口事件处理

use crate::runner::RunnerInner;
use lumino_ui::event::window::sync::Event;

impl RunnerInner {
    pub(crate) fn handle_sync_events(&mut self, window_event: Event) {
        use lumino_ui::event::window::sync::Event::*;
        match window_event {
            LocalNoteAdded {
                id,
                tick,
                key,
                length,
                velocity,
                channel,
                track_index,
            } => {
                self.handle_local_note_added(id, tick, key, length, velocity, channel, track_index);
            }
            LocalNoteMoved {
                id,
                tick,
                key,
                length,
                tick_offset,
                key_offset,
                track_index,
            } => {
                self.handle_local_note_moved(
                    id,
                    tick,
                    key,
                    length,
                    tick_offset,
                    key_offset,
                    track_index,
                );
            }
            LocalNoteDeleted {
                id,
                tick,
                key,
                length,
                velocity,
                channel,
                track_index,
            } => {
                self.handle_local_note_deleted(id, tick, key, length, velocity, channel, track_index);
            }
            LocalTrackAdded { track_index } => {
                self.handle_local_track_added(track_index);
            }
            LocalSelectionChanged {
                active,
                timestamp,
                fingerprints,
            } => {
                self.handle_local_selection_changed(active, timestamp, fingerprints);
            }
        }
    }
}
