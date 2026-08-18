//! 力度点构建单元测试

use crate::Note;
use crate::velocity::VelocityPanel;

/// 将 f32 Note 转为 NoteEvent（测试辅助，与 document 存储格式一致）
fn to_events(notes: &[Note]) -> Vec<lumino_midi_loader::NoteEvent> {
    notes
        .iter()
        .map(|n| {
            lumino_midi_loader::NoteEvent::new(
                n.tick.round() as u32,
                (n.tick + n.length).round() as u32,
                n.key as u8,
                n.velocity,
                n.channel,
            )
        })
        .collect()
}

#[test]
fn test_build_velocity_points_empty() {
    let notes: lumino_midi_loader::ChunkedList<lumino_midi_loader::NoteEvent> =
        lumino_midi_loader::ChunkedList::new();
    let points = VelocityPanel::build_velocity_points(&notes);
    assert!(points.is_empty());
}

#[test]
fn test_build_velocity_points_single_note() {
    let notes =
        lumino_midi_loader::ChunkedList::from_sorted(to_events(&[
            Note::new(0.0, 60, 480.0).with_velocity(100)
        ]));
    let points = VelocityPanel::build_velocity_points(&notes);

    assert_eq!(points.len(), 1);
    assert_eq!(points[0].note_index, 0);
    assert_eq!(points[0].tick, 0.0);
    assert_eq!(points[0].velocity, 100);
}

#[test]
fn test_build_velocity_points_multiple_notes() {
    let notes = lumino_midi_loader::ChunkedList::from_sorted(to_events(&[
        Note::new(480.0, 64, 240.0).with_velocity(80),
        Note::new(0.0, 60, 480.0).with_velocity(100),
        Note::new(960.0, 67, 240.0).with_velocity(120),
        Note::new(480.0, 72, 120.0).with_velocity(60),
    ]));

    let points = VelocityPanel::build_velocity_points(&notes);
    assert_eq!(points.len(), 4);
    assert_eq!(points[0].tick, 0.0);
    assert_eq!(points[0].note_index, 1);
    assert_eq!(points[1].tick, 480.0);
    assert_eq!(points[1].note_index, 0);
    assert_eq!(points[2].tick, 480.0);
    assert_eq!(points[2].note_index, 3);
    assert_eq!(points[3].tick, 960.0);
    assert_eq!(points[3].note_index, 2);
}
