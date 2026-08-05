#[allow(clippy::unwrap_used)]
mod tests {
    use crate::EditorData;
    use lumino_note_core::note::Note;
    use lumino_note_core::note_store::BitSet;

    #[test]
    fn test_sync_note_store_auto_enable() {
        let notes: Vec<Note> = (0..100).map(|i| Note::new(i as f32, 60, 1.0)).collect();
        let mut editor_data = EditorData::with_f32_notes(1, &notes);
        editor_data.sync_note_store();
        // NoteStore 已降级：恒不启用
        assert!(!editor_data.is_note_store_enabled());
    }

    #[test]
    fn test_batch_move_cold_path() {
        let notes: Vec<Note> = (0..5)
            .map(|note_idx| Note::new(note_idx as f32 * 10.0, 60, 1.0))
            .collect();
        let mut editor_data = EditorData::with_f32_notes(1, &notes);

        let mut sel = BitSet::new(5);
        sel.set(0);
        sel.set(2);

        let modified = editor_data.batch_move_notes(&sel, 10.0, 3, 127);
        assert_eq!(modified, 2);
        assert_eq!(editor_data.get_note_view(0).unwrap().tick, 10.0);
        assert_eq!(editor_data.get_note_view(0).unwrap().key, 63);
        assert_eq!(
            editor_data.get_note_view(1).unwrap().tick,
            10.0,
            "未选中不变"
        );
        assert_eq!(editor_data.get_note_view(2).unwrap().tick, 30.0);
    }

    #[test]
    fn test_batch_move_large() {
        // 大数据量走同一降级路径，验证正确性
        let notes: Vec<Note> = (0..10_100)
            .map(|note_idx| Note::new(note_idx as f32, 60, 1.0))
            .collect();
        let mut editor_data = EditorData::with_f32_notes(1, &notes);
        editor_data.sync_note_store();

        let note_count = editor_data.current_track_note_count();
        let mut sel = BitSet::new(note_count);
        for note_idx in (0..note_count).step_by(2) {
            sel.set(note_idx);
        }

        let modified = editor_data.batch_move_notes(&sel, 5.0, 2, 127);
        assert_eq!(modified, (note_count + 1) / 2);

        assert_eq!(editor_data.get_note_view(0).unwrap().tick, 5.0);
        assert_eq!(
            editor_data.get_note_view(1).unwrap().tick,
            1.0,
            "未选中不变"
        );
    }

    #[test]
    fn test_batch_delete() {
        let notes: Vec<Note> = (0..10)
            .map(|note_idx| Note::new(note_idx as f32 * 10.0, 60, 1.0))
            .collect();
        let mut editor_data = EditorData::with_f32_notes(1, &notes);

        let mut sel = BitSet::new(10);
        sel.set(2);
        sel.set(5);
        sel.set(8);

        let deleted = editor_data.batch_delete_notes(&sel);
        assert_eq!(deleted, 3);
        assert_eq!(editor_data.current_track_note_count(), 7);
        assert_eq!(editor_data.get_note_view(0).unwrap().tick, 0.0);
        assert_eq!(editor_data.get_note_view(1).unwrap().tick, 10.0);
        assert_eq!(editor_data.get_note_view(2).unwrap().tick, 30.0);
    }

    #[test]
    fn test_batch_insert() {
        let mut editor_data = EditorData::with_f32_notes(1, &[Note::new(0.0, 60, 1.0)]);

        let new_notes = vec![
            Note::new(100.0, 62, 2.0),
            Note::new(200.0, 64, 3.0),
            Note::new(300.0, 66, 4.0),
        ];

        let inserted = editor_data.batch_insert_notes(&new_notes);
        assert_eq!(inserted, 3);
        assert_eq!(editor_data.current_track_note_count(), 4);
        assert_eq!(editor_data.get_note_view(1).unwrap().tick, 100.0);
        assert_eq!(editor_data.get_note_view(3).unwrap().tick, 300.0);
    }
}
