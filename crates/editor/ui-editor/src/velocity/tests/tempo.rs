//! Tempo 数据与延伸单元测试

use crate::editor_state::ViewState;
use crate::velocity::VelocityPanel;
use crate::velocity::widget;

#[test]
fn test_tempo_bpm_to_y_density_uniform() {
    let height = 200.0;
    let max_bpm = 512.0;
    let levels = widget::generate_tempo_levels(max_bpm);
    assert_eq!(levels.len(), 9);

    let mut spacings = Vec::new();
    let mut prev_y = widget::tempo_bpm_to_y(levels[0], max_bpm, height);
    for &bpm in levels.iter().skip(1) {
        let point_y = widget::tempo_bpm_to_y(bpm, max_bpm, height);
        spacings.push((prev_y - point_y).abs());
        prev_y = point_y;
    }

    let first = spacings[0];
    for &spacing in &spacings {
        assert!(
            (spacing - first).abs() < f32::EPSILON,
            "等差刻度应在 Y 轴上均匀分布：spacing={spacing}, first={first}"
        );
    }
}

/// 不同 BPM 上限下刻度生成应与上限联动
#[test]
fn test_tempo_levels_scale_with_max_bpm() {
    let levels_512 = widget::generate_tempo_levels(512.0);
    let levels_1024 = widget::generate_tempo_levels(1024.0);
    let last_512 = *levels_512.last().expect("刻度数量固定为 9，last 必存在");
    let last_1024 = *levels_1024.last().expect("刻度数量固定为 9，last 必存在");
    assert_eq!(last_512, 512.0);
    assert_eq!(last_1024, 1024.0);
    // 512 上限的刻度全部落在 1024 上限的范围内
    assert!(levels_512.iter().all(|bpm| *bpm <= last_1024));
}

#[test]
fn test_tempo_point_screen_pos_matches_bpm_to_y() {
    let height = 200.0;
    let view = ViewState::default();
    let point = widget::TempoPoint {
        tick: 0.0,
        bpm: 120.0,
    };
    let max_bpm = 512.0;
    let pos = widget::tempo_point_screen_pos(&point, height, &view, max_bpm);
    let expected_y = widget::tempo_bpm_to_y(120.0, max_bpm, height);
    assert!((pos.y - expected_y).abs() < f32::EPSILON);
}

// ===== Tempo 折线无限水平延伸测试 =====

/// 辅助：构造 ViewState（keyboard_width/zoom_x/scroll_x）
fn tempo_view(keyboard_width: f32, zoom_x: f32, scroll_x: f32) -> ViewState {
    ViewState {
        keyboard_width,
        zoom_x,
        scroll_x,
        ..ViewState::default()
    }
}

/// 单点（默认 tick=0）时：从第一个点起向右无限水平延伸
#[test]
fn test_tempo_extension_single_point_extends_right() {
    let points = vec![widget::TempoPoint {
        tick: 0.0,
        bpm: 120.0,
    }];
    let view = tempo_view(80.0, 0.5, 0.0);
    let end = widget::tempo_extension_end(&points, 800.0, 200.0, &view, 512.0)
        .expect("单点时应产生延伸终点");
    // 起点 x = 0*0.5 - 0 + 80 = 80 < 850 → 延伸至视口右边缘外
    assert_eq!(end.x, 800.0 + 50.0);
    // 延伸段保持最后一个点的 BPM 高度
    let expect_y = widget::tempo_bpm_to_y(120.0, 512.0, 200.0);
    assert!((end.y - expect_y).abs() < f32::EPSILON);
}

/// 多个点时：从最后一个 tempo 点继续向后无限水平延伸
#[test]
fn test_tempo_extension_last_point_extends_right() {
    let points = vec![
        widget::TempoPoint {
            tick: 0.0,
            bpm: 120.0,
        },
        widget::TempoPoint {
            tick: 480.0,
            bpm: 200.0,
        },
    ];
    let view = tempo_view(80.0, 0.5, 0.0);
    let end = widget::tempo_extension_end(&points, 800.0, 200.0, &view, 512.0)
        .expect("最后一个点在视口内时应产生延伸终点");
    // 最后一个点 x = 480*0.5 - 0 + 80 = 320 < 850 → 延伸
    assert_eq!(end.x, 800.0 + 50.0);
    // 延伸段保持最后一个点（200 BPM）的高度
    let expect_y = widget::tempo_bpm_to_y(200.0, 512.0, 200.0);
    assert!((end.y - expect_y).abs() < f32::EPSILON);
}

/// 最后一个 tempo 点已在视口右侧之外：延伸段不可见，不绘制
#[test]
fn test_tempo_extension_no_extension_when_last_outside_viewport() {
    let points = vec![
        widget::TempoPoint {
            tick: 0.0,
            bpm: 120.0,
        },
        widget::TempoPoint {
            tick: 5000.0,
            bpm: 200.0,
        },
    ];
    let view = tempo_view(80.0, 0.5, 0.0);
    // 最后一个点 x = 5000*0.5 = 2500 > 850 → 不延伸
    let end = widget::tempo_extension_end(&points, 800.0, 200.0, &view, 512.0);
    assert!(end.is_none());
}

/// 空点列表：无延伸
#[test]
fn test_tempo_extension_empty_points() {
    let view = tempo_view(80.0, 0.5, 0.0);
    let end = widget::tempo_extension_end(&[], 800.0, 200.0, &view, 512.0);
    assert!(end.is_none());
}

#[test]
fn test_build_tempo_points_no_document() {
    use crate::Editor;
    let editor = Editor::new();
    let points = VelocityPanel::build_tempo_points(&editor);
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].tick, 0.0);
    assert!((points[0].bpm - 120.0).abs() < 0.01);
}

#[test]
fn test_build_tempo_points_from_editor_data() {
    use crate::Editor;
    let mut editor = Editor::new();
    // 直接向 tempo_points 写入数据模拟已加载文档
    editor.editor_state.data.tempo_points = vec![
        lumino_note_core::TempoPoint {
            tick: 0.0,
            bpm: 120.0,
        },
        lumino_note_core::TempoPoint {
            tick: 480.0,
            bpm: 140.0,
        },
    ];

    let points = VelocityPanel::build_tempo_points(&editor);
    // 现在 build_tempo_points 从 data.tempo_points 读取，返回编辑后的数据
    assert_eq!(points.len(), 2);
    assert_eq!(points[0].tick, 0.0);
    assert!((points[0].bpm - 120.0).abs() < 0.01);
    assert_eq!(points[1].tick, 480.0);
    assert!((points[1].bpm - 140.0).abs() < 0.01);
}
