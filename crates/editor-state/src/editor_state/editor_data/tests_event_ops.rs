//! 事件浏览器 CRUD 操作测试

use std::collections::HashSet;

use lumino_note_core::event::{
    AutomationTarget, ChordEvent, KeySignatureEvent, LyricsEvent, MarkerEvent, ProgramChangeEvent,
    ScaleType, SegmentShape, TimeSignatureEvent,
};

use super::EditorData;

#[test]
fn test_time_sig_crud() {
    let mut data = EditorData::new();
    data.set_time_sig_event(0, 4, 4);
    data.set_time_sig_event(960, 3, 4);
    data.insert_time_sig_event(480);

    assert_eq!(data.time_signatures.len(), 3);
    assert_eq!(data.time_signatures[0], (0, 4, 4));
    assert_eq!(data.time_signatures[1], (480, 4, 4));
    assert_eq!(data.time_signatures[2], (960, 3, 4));

    let mut ticks = HashSet::new();
    ticks.insert(0);
    data.delete_time_sig_events(&ticks);
    assert_eq!(data.time_signatures.len(), 2);
    assert_eq!(data.time_signatures[0], (480, 4, 4));
}

#[test]
fn test_key_sig_crud() {
    let mut data = EditorData::new();
    data.set_key_sig_event(0, 0, ScaleType::Major);
    data.set_key_sig_event(960, 7, ScaleType::Minor);
    data.insert_key_sig_event(480);

    assert_eq!(data.key_signatures.len(), 3);
    assert_eq!(data.key_signatures[0].tick, 0);
    assert_eq!(data.key_signatures[1].tick, 480);
    assert_eq!(data.key_signatures[2].scale, ScaleType::Minor);

    let mut ticks = HashSet::new();
    ticks.insert(0);
    data.delete_key_sig_events(&ticks);
    assert_eq!(data.key_signatures.len(), 2);
}

#[test]
fn test_marker_crud() {
    let mut data = EditorData::new();
    data.set_marker_event(0, "Intro".into());
    data.insert_marker_event(480);

    assert_eq!(data.markers.len(), 2);
    assert_eq!(data.markers[0].text, "Intro");
    assert_eq!(data.markers[1].text, "New");

    let mut ticks = HashSet::new();
    ticks.insert(480);
    data.delete_marker_events(&ticks);
    assert_eq!(data.markers.len(), 1);
}

#[test]
fn test_lyrics_crud() {
    let mut data = EditorData::new();
    data.set_lyrics_event(0, 0, "La".into());
    data.insert_lyrics_event(0, 480);

    assert_eq!(data.lyrics.len(), 2);
    assert_eq!(data.lyrics[0].text, "La");
    assert_eq!(data.lyrics[1].text, "");

    let mut ticks = HashSet::new();
    ticks.insert(0);
    data.delete_lyrics_events(0, &ticks);
    assert_eq!(data.lyrics.len(), 1);
}

#[test]
fn test_lyrics_per_track_isolation() {
    let mut data = EditorData::new();
    // 同一 tick 不同音轨：互不覆盖
    data.set_lyrics_event(1, 480, "Lead 歌词".into());
    data.set_lyrics_event(2, 480, "Bass 歌词".into());
    assert_eq!(data.lyrics.len(), 2);

    // 替换同 (track, tick) 事件
    data.set_lyrics_event(1, 480, "Lead 更新".into());
    assert_eq!(data.lyrics.len(), 2);
    assert_eq!(
        data.lyrics
            .iter()
            .find(|e| e.track == 1)
            .map(|e| e.text.as_str()),
        Some("Lead 更新")
    );

    // 删除仅影响指定音轨
    let mut ticks = HashSet::new();
    ticks.insert(480);
    data.delete_lyrics_events(1, &ticks);
    assert_eq!(data.lyrics.len(), 1);
    assert_eq!(data.lyrics[0].track, 2);
}

#[test]
fn test_chord_crud() {
    let mut data = EditorData::new();
    data.set_chord_event(0, 0, "C".into());
    data.insert_chord_event(0, 480);

    assert_eq!(data.chords.len(), 2);
    assert_eq!(data.chords[0].text, "C");
    assert_eq!(data.chords[1].text, "");

    let mut ticks = HashSet::new();
    ticks.insert(0);
    data.delete_chord_events(0, &ticks);
    assert_eq!(data.chords.len(), 1);
}

#[test]
fn test_program_change_crud() {
    let mut data = EditorData::new();
    data.set_program_change_event(0, 0, 5);
    data.insert_program_change_event(0, 480);

    assert_eq!(data.program_changes.len(), 2);
    assert_eq!(data.program_changes[0].program, 5);
    assert_eq!(data.program_changes[1].program, 0);

    let mut ticks = HashSet::new();
    ticks.insert(0);
    data.delete_program_change_events(0, &ticks);
    assert_eq!(data.program_changes.len(), 1);
}

#[test]
fn test_program_change_per_track_isolation() {
    let mut data = EditorData::new();
    data.set_program_change_event(1, 480, 5);
    data.set_program_change_event(2, 480, 40);
    assert_eq!(data.program_changes.len(), 2);

    let mut ticks = HashSet::new();
    ticks.insert(480);
    data.delete_program_change_events(1, &ticks);
    assert_eq!(data.program_changes.len(), 1);
    assert_eq!(data.program_changes[0].track, 2);
}

#[test]
fn test_automation_cc_crud() {
    let mut data = EditorData::new();
    data.set_automation_event(1, AutomationTarget::Cc(7), 0, 100.0, SegmentShape::Step);

    let idx = data
        .find_automation_lane(
            1,
            &lumino_note_core::automation::AutomationTarget::CC { controller: 7 },
        )
        .expect("应存在 volume lane");
    assert_eq!(data.automation_lanes[idx].events.len(), 1);
    assert_eq!(data.automation_lanes[idx].events[0].value, 100);

    let mut ticks = HashSet::new();
    ticks.insert(0);
    data.delete_automation_events(1, &AutomationTarget::Cc(7), &ticks);
    assert!(data.automation_lanes[idx].events.is_empty());

    data.insert_automation_event(1, &AutomationTarget::Cc(7), 480);
    assert_eq!(data.automation_lanes[idx].events.len(), 1);
    assert_eq!(data.automation_lanes[idx].events[0].value, 0);
}

#[test]
fn test_automation_tempo() {
    let mut data = EditorData::new();
    data.set_automation_event(0, AutomationTarget::Tempo, 0, 140.0, SegmentShape::Step);
    assert_eq!(data.tempo_points.len(), 1);
    assert!((data.tempo_points[0].bpm - 140.0).abs() < f64::EPSILON);

    let mut ticks = HashSet::new();
    ticks.insert(0);
    data.delete_automation_events(0, &AutomationTarget::Tempo, &ticks);
    assert!(data.tempo_points.is_empty());
}

/// 正向（tension → 控制点）与反向（控制点 → tension）互逆性测试。
///
/// 正向映射见 `detail/auto.rs::lane_shape_to_event_shape`：
/// - `t >= 0`：y1 = 0，y2 = 0.5t
/// - `t < 0`：y1 = -0.5t，y2 = 1
#[test]
fn test_curve_tension_roundtrip() {
    use super::shape_convert::curve_to_tension;

    // 与 UI 正向映射一致的控制点 → 应反算出原 tension（±1 容差）
    let cases = [
        (0i8, (0.25, 0.0, 0.75, 1.0)),
        (127i8, (0.25, 0.0, 0.75, 0.5)),
        (-127i8, (0.25, 0.5, 0.75, 1.0)),
        (64i8, (0.25, 0.0, 0.75, 0.25)),
        (-64i8, (0.25, 0.25, 0.75, 1.0)),
        (32i8, (0.25, 0.0, 0.75, 0.125)),
        (-32i8, (0.25, 0.125, 0.75, 1.0)),
    ];
    for (tension, (x1, y1, x2, y2)) in cases {
        let back = curve_to_tension(x1, y1, x2, y2);
        assert!(
            (back - tension).abs() <= 1,
            "tension {tension} → 控制点 → 反算 {back}，误差应 ≤1"
        );
    }
}

/// 任意贝塞尔控制点（非流形）也映射到合法 tension 范围。
#[test]
fn test_curve_to_tension_generic_points() {
    use super::shape_convert::curve_to_tension;
    let t = curve_to_tension(0.1, 0.2, 0.9, 0.7);
    assert!((-127..=127).contains(&t));
    // 中心对称控制点 → 接近 0
    let t2 = curve_to_tension(0.25, 0.4, 0.75, 0.6);
    assert!(t2.abs() <= 2, "对称控制点应接近直线，tension = {t2}");
}

/// 编辑自动化事件时保留曲线形状：set_automation_event 中 Curve 控制点
/// 反算为 lane tension（不再是固定 0）。
#[test]
fn test_set_automation_preserves_curve_tension() {
    use lumino_note_core::automation::SegmentShape as LaneShape;
    let mut data = EditorData::new();
    // ease-in 曲线（tension 正向 → y2 < 1）
    data.set_automation_event(
        1,
        AutomationTarget::Cc(7),
        0,
        100.0,
        SegmentShape::Curve {
            x1: 0.25,
            y1: 0.0,
            x2: 0.75,
            y2: 0.5,
        },
    );
    let idx = data
        .find_automation_lane(
            1,
            &lumino_note_core::automation::AutomationTarget::CC { controller: 7 },
        )
        .expect("应存在 volume lane");
    match data.automation_lanes[idx].events[0].shape {
        LaneShape::Curve { tension } => {
            assert!(
                tension > 100,
                "ease-in 曲线 tension 应接近 127，实际 {tension}"
            )
        }
        LaneShape::Step => panic!("曲线控制点不应映射为 Step"),
    }
}

#[test]
fn test_insert_note_on_nonzero_track() {
    // document 唯一权威：需要先构造含 track 1 的 document
    let mut data = EditorData::with_f32_notes(1, &[]);
    data.current_track = 0;
    assert!(data.insert_note_at_tick(100.0).is_none());

    data.current_track = 1;
    let note = data.insert_note_at_tick(100.0).expect("应成功插入音符");
    assert_eq!(note.tick, 100.0);
    assert_eq!(note.key, 60);
    assert_eq!(note.length, 480.0);
    assert_eq!(note.velocity, 100);
    assert_eq!(data.current_track_note_count(), 1);
}

#[test]
fn test_blank_project_can_insert_note() {
    // 2026-08 回归：空白工程（新建文件）必须立即可创建音符。
    // 模拟 init_blank_project 后的状态：空 document（默认 2 轨）+ 当前轨 = Setup(1)。
    let mut data = EditorData::new();
    let doc = lumino_midi_model::MidiDocument::empty_with_tracks(2);
    data.document = Some(doc);
    data.current_track = 1;

    // 直接插入（arrange_add_note 路径：insert_note）
    let note = lumino_note_core::note::Note::new(0.0, 60, 480.0);
    assert!(data.insert_note(1, note.clone()));
    assert_eq!(data.current_track_note_count(), 1);

    // insert_note_at_tick 路径（钢琴卷帘）
    let n2 = data
        .insert_note_at_tick(100.0)
        .expect("空白工程应能创建音符");
    assert_eq!(n2.tick, 100.0);
    assert_eq!(data.current_track_note_count(), 2);
}

#[test]
fn test_delete_notes_at_ticks() {
    let mut data = EditorData::with_f32_notes(1, &[]);
    data.current_track = 1;
    data.insert_note_at_tick(100.0);
    data.insert_note_at_tick(200.0);
    data.insert_note_at_tick(300.0);
    assert_eq!(data.current_track_note_count(), 3);

    let mut ticks = HashSet::new();
    ticks.insert(100);
    ticks.insert(300);
    data.delete_notes_at_ticks(&ticks);

    assert_eq!(data.current_track_note_count(), 1);
    assert_eq!(data.get_note_view(0).unwrap().tick, 200.0);
}

#[test]
fn test_reset_clears_event_fields() {
    let mut data = EditorData::new();
    data.set_marker_event(0, "A".into());
    data.set_key_sig_event(0, 0, ScaleType::Major);
    data.reset();
    assert!(data.markers.is_empty());
    assert!(data.key_signatures.is_empty());
    assert!(data.lyrics.is_empty());
    assert!(data.chords.is_empty());
    assert!(data.program_changes.is_empty());
}
