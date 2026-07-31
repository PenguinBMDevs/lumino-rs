#[allow(clippy::unwrap_used)]
mod tests {
    use super::super::EditorData;
    use super::super::NOTE_STORE_THRESHOLD;
    use crate::note::Note;
    use crate::note_store::BitSet;

    #[test]
    fn test_sync_note_store_auto_enable() {
        let mut editor_data = EditorData::new();
        for note_idx in 0..100 {
            editor_data
                .notes
                .push_back(Note::new(note_idx as f32, 60, 1.0));
        }
        editor_data.sync_note_store();
        assert!(!editor_data.is_note_store_enabled());

        for note_idx in 0..NOTE_STORE_THRESHOLD {
            editor_data
                .notes
                .push_back(Note::new(note_idx as f32, 60, 1.0));
        }
        editor_data.sync_note_store();
        assert!(editor_data.is_note_store_enabled());
        assert_eq!(editor_data.note_store.len(), editor_data.notes.len());
    }

    #[test]
    fn test_batch_move_cold_path() {
        let mut editor_data = EditorData::new();
        editor_data.current_track = 1;
        for note_idx in 0..5 {
            editor_data
                .notes
                .push_back(Note::new(note_idx as f32 * 10.0, 60, 1.0));
        }
        editor_data.sync_track_notes();

        let mut sel = BitSet::new(5);
        sel.set(0);
        sel.set(2);

        let modified = editor_data.batch_move_notes(&sel, 10.0, 3, 127);
        assert_eq!(modified, 2);
        assert_eq!(editor_data.notes[0].tick, 10.0);
        assert_eq!(editor_data.notes[0].key, 63);
        assert_eq!(editor_data.notes[1].tick, 10.0, "未选中不变");
        assert_eq!(editor_data.notes[2].tick, 30.0);
    }

    #[test]
    fn test_batch_move_hot_path() {
        let mut editor_data = EditorData::new();
        editor_data.current_track = 1;
        for note_idx in 0..NOTE_STORE_THRESHOLD + 100 {
            editor_data
                .notes
                .push_back(Note::new(note_idx as f32, 60, 1.0));
        }
        editor_data.sync_note_store();
        assert!(editor_data.is_note_store_enabled());

        let mut sel = BitSet::new(editor_data.notes.len());
        for note_idx in (0..editor_data.notes.len()).step_by(2) {
            sel.set(note_idx);
        }

        let modified = editor_data.batch_move_notes(&sel, 5.0, 2, 127);
        assert_eq!(modified, (editor_data.notes.len() + 1) / 2);

        assert_eq!(editor_data.notes.len(), editor_data.note_store.len());
        assert_eq!(editor_data.notes[0].tick, 5.0);
        assert_eq!(editor_data.notes[1].tick, 1.0, "未选中不变");
    }

    #[test]
    fn test_batch_delete() {
        let mut editor_data = EditorData::new();
        editor_data.current_track = 1;
        for note_idx in 0..10 {
            editor_data
                .notes
                .push_back(Note::new(note_idx as f32 * 10.0, 60, 1.0));
        }
        editor_data.sync_track_notes();

        let mut sel = BitSet::new(10);
        sel.set(2);
        sel.set(5);
        sel.set(8);

        let deleted = editor_data.batch_delete_notes(&sel);
        assert_eq!(deleted, 3);
        assert_eq!(editor_data.notes.len(), 7);
        assert_eq!(editor_data.notes[0].tick, 0.0);
        assert_eq!(editor_data.notes[1].tick, 10.0);
        assert_eq!(editor_data.notes[2].tick, 30.0);
    }

    #[test]
    fn test_batch_insert() {
        let mut editor_data = EditorData::new();
        editor_data.current_track = 1;
        editor_data.notes.push_back(Note::new(0.0, 60, 1.0));
        editor_data.sync_track_notes();

        let new_notes = vec![
            Note::new(100.0, 62, 2.0),
            Note::new(200.0, 64, 3.0),
            Note::new(300.0, 66, 4.0),
        ];

        let inserted = editor_data.batch_insert_notes(&new_notes);
        assert_eq!(inserted, 3);
        assert_eq!(editor_data.notes.len(), 4);
        assert_eq!(editor_data.notes[1].tick, 100.0);
        assert_eq!(editor_data.notes[3].tick, 300.0);
    }

    #[test]
    fn test_consistency_after_operations() {
        let mut editor_data = EditorData::new();
        editor_data.current_track = 1;
        for note_idx in 0..NOTE_STORE_THRESHOLD + 50 {
            editor_data.notes.push_back(Note::new(
                note_idx as f32,
                60 + (note_idx % 12) as u16,
                1.0,
            ));
        }
        editor_data.sync_note_store();
        assert!(editor_data.is_note_store_enabled());

        let mut sel = BitSet::new(editor_data.notes.len());
        for note_idx in (0..editor_data.notes.len()).step_by(2) {
            sel.set(note_idx);
        }
        let moved = editor_data.batch_move_notes(&sel, 10.0, 3, 127);
        assert!(moved > 0);
        assert_eq!(editor_data.notes.len(), editor_data.note_store.len());

        let mut sel_del = BitSet::new(editor_data.notes.len());
        for note_idx in (0..editor_data.notes.len()).step_by(4) {
            sel_del.set(note_idx);
        }
        let before = editor_data.notes.len();
        let deleted = editor_data.batch_delete_notes(&sel_del);
        assert_eq!(deleted, (before + 3) / 4);
        assert_eq!(editor_data.notes.len(), editor_data.note_store.len());

        let new_notes: Vec<Note> = (0..100)
            .map(|idx| Note::new(idx as f32 * 5.0, 70, 2.0))
            .collect();
        let before_len = editor_data.notes.len();
        let inserted = editor_data.batch_insert_notes(&new_notes);
        assert_eq!(inserted, 100);
        assert_eq!(editor_data.notes.len(), before_len + 100);
        assert_eq!(editor_data.notes.len(), editor_data.note_store.len());
    }
}
