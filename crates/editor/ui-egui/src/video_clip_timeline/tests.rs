//! 时间轴单测：命中、拖拽绝对值语义、滚轮、刻度与几何。

use super::*;

fn model() -> VideoTimelineModel {
    VideoTimelineModel {
        duration_secs: 30.0,
        zoom: 1.0,
        scroll_x: 0.0,
        playhead_secs: 0.0,
        video: ClipEdit::default(),
        audio: ClipEdit::default(),
        viewport_w: 400.0,
        title_row: None,
        scrollbar: None,
        content_w: 0.0,
        title: "剪辑带".to_string(),
        info: String::new(),
        playing: false,
        rewind_tooltip: String::new(),
        play_tooltip: String::new(),
        pause_tooltip: String::new(),
    }
}

#[test]
fn press_on_ruler_seeks_and_opens_session() {
    let m = model();
    let (action, session) = press(egui::pos2(80.0, 10.0), &m, 400.0);
    // 80px / 80pps = 1s。
    assert!(
        matches!(action, Some(VideoTimelineAction::Seek(s)) if (s - 1.0).abs() < 1e-3),
        "标尺按下应 Seek 到 1s，实际 {action:?}"
    );
    assert!(matches!(session.map(|s| s.zone), Some(HitZone::Ruler)));
}

#[test]
fn drag_uses_zone_semantics_with_absolute_values() {
    let m = model();
    let y_body = TrackGeom::track_y(ClipTrack::Video) + TRACK_HEIGHT / 2.0;
    // 条身拖动：基准 offset=0，右移 160px = +2s → 绝对 offset=2s。
    let session = DragSession {
        zone: HitZone::Body(ClipTrack::Video),
        origin: egui::pos2(300.0, y_body),
        base: 0.0,
    };
    let action = drag(egui::pos2(460.0, y_body), session, &m);
    assert!(
        matches!(action, Some(VideoTimelineAction::SetOffset { track, offset })
            if track == ClipTrack::Video && (offset - 2.0).abs() < 1e-3),
        "条身拖动应产出绝对 offset=+2s，实际 {action:?}"
    );

    // 起始把手：基准 trim_in=1s，右移 80px = +1s → trim_in=2s。
    let head = DragSession {
        zone: HitZone::Head(ClipTrack::Video),
        origin: egui::pos2(8.0, y_body),
        base: 1.0,
    };
    let action = drag(egui::pos2(88.0, y_body), head, &m);
    assert!(
        matches!(action, Some(VideoTimelineAction::SetTrim { edge, secs, .. })
            if edge == ClipEdge::Start && (secs - 2.0).abs() < 1e-3),
        "起始把手应产出 trim_in=2s，实际 {action:?}"
    );

    // 结束把手：基准 trim_out=0，左移 80px → trim_out=+1s。
    let tail = DragSession {
        zone: HitZone::Tail(ClipTrack::Video),
        origin: egui::pos2(300.0, y_body),
        base: 0.0,
    };
    let action = drag(egui::pos2(220.0, y_body), tail, &m);
    assert!(
        matches!(action, Some(VideoTimelineAction::SetTrim { edge, secs, .. })
            if edge == ClipEdge::End && (secs - 1.0).abs() < 1e-3),
        "结束把手左移应产出 trim_out=1s，实际 {action:?}"
    );
}

#[test]
fn wheel_ctrl_zooms_otherwise_scrolls() {
    let zoom = wheel(egui::pos2(100.0, 10.0), 1.0, true, false, 400.0);
    assert!(
        matches!(zoom, Some(VideoTimelineAction::Zoom { factor, fixed_ratio })
            if (factor - 1.1).abs() < 1e-4 && (fixed_ratio - 0.25).abs() < 1e-4),
        "Ctrl+滚轮应产出 ×1.1、锚点 0.25，实际 {zoom:?}"
    );
    let scroll = wheel(egui::pos2(100.0, 10.0), 1.0, false, false, 400.0);
    assert!(
        matches!(scroll, Some(VideoTimelineAction::ScrollX(d)) if (d + 60.0).abs() < 1e-3),
        "普通滚轮应横向滚动 -60px，实际 {scroll:?}"
    );
}

#[test]
fn ticks_follow_major_and_minor_intervals() {
    let ticks = ticks(&model(), 500.0);
    let majors: Vec<f32> = ticks.iter().filter(|(_, m)| *m).map(|(t, _)| *t).collect();
    assert!(
        majors.contains(&0.0) && majors.contains(&5.0),
        "主刻度每 5 秒"
    );
    let minors = ticks.len() - majors.len();
    assert!(minors >= 2, "次刻度每 1 秒");
}

#[test]
fn hit_test_splits_ruler_body_and_handles() {
    let m = model();
    // 标尺：y < 24。
    assert_eq!(
        hit_test(egui::pos2(100.0, 10.0), &m, 400.0),
        Some(HitZone::Ruler)
    );
    // 视频条：y = 24+8 .. +48，条宽 30s*80 = 2400（宽于视口）。
    let y = TrackGeom::track_y(ClipTrack::Video) + TRACK_HEIGHT / 2.0;
    assert_eq!(
        hit_test(egui::pos2(300.0, y), &m, 400.0),
        Some(HitZone::Body(ClipTrack::Video))
    );
    // 起始把手：x <= 16。
    assert_eq!(
        hit_test(egui::pos2(8.0, y), &m, 400.0),
        Some(HitZone::Head(ClipTrack::Video))
    );
    // 音频条。
    let ya = TrackGeom::track_y(ClipTrack::Audio) + TRACK_HEIGHT / 2.0;
    assert_eq!(
        hit_test(egui::pos2(300.0, ya), &m, 400.0),
        Some(HitZone::Body(ClipTrack::Audio))
    );
    // 空白处。
    assert_eq!(hit_test(egui::pos2(300.0, 300.0), &m, 400.0), None);
}

#[test]
fn track_geometry_uses_zoom_and_scroll() {
    let g = TrackGeom::new((10.0, 5.0), 160.0, 100.0, 0.0);
    assert!((g.x - (10.0 * 160.0 - 100.0)).abs() < 0.01);
    assert!((g.w - 5.0 * 160.0).abs() < 0.01);
}
