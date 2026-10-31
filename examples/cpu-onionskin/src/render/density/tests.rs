use super::*;

fn index(total_notes: u64, measure_ticks: u32) -> TrackIndex {
    TrackIndex {
        max_len: vec![0],
        short_note_ticks: (measure_ticks / 16).max(1),
        long_notes: vec![Vec::new()],
        long_note_count: 0,
        measure_counts: vec![0],
        measure_ticks,
        densest_measure: 0,
        densest_measure_notes: 0,
        total_notes,
        key_masks: Vec::new(),
        key_mask_tracks: 0,
        key_mask_measures: 0,
    }
}

fn camera(zoom_x: f32) -> Camera {
    let mut camera = Camera::new(1920, 1080, 1.0, 1_000_000, 480, zoom_x);
    camera.zoom_x = zoom_x;
    // 本文件只验证小节数判定，去掉键盘栏让可见宽度恰好 1920px。
    camera.keyboard_width = 0.0;
    camera
}

#[test]
fn overview_switches_at_thirty_visible_measures() {
    let index = index(1, 1920);
    let mut camera = camera(0.0);
    camera.zoom_y = 20.0;
    // 相机宽 1920px；可见 tick 跨度 = 小节数 × 1920。
    camera.zoom_x = 1920.0 / (29.0 * 1920.0);
    assert_eq!(visible_measure_count(&index, &camera), 29);
    assert!(!uses_overview(&index, &camera), "29 小节必须 Exact");

    camera.zoom_x = 1920.0 / (30.0 * 1920.0);
    assert_eq!(visible_measure_count(&index, &camera), 30);
    assert!(uses_overview(&index, &camera), "30 小节切总览");

    camera.zoom_x = 1920.0 / (31.0 * 1920.0);
    assert!(uses_overview(&index, &camera), "31 小节继续总览");
}

#[test]
fn overview_ignores_note_count_short_notes_and_key_height() {
    // 1 个音符、1-tick 短音符、0.5px 键行，只要 <30 小节就必须 Exact。
    let mut short_index = index(1, 1920);
    short_index.short_note_ticks = 1;
    let mut camera = camera(1920.0 / (29.0 * 1920.0));
    camera.zoom_y = 0.5;
    assert!(!uses_overview(&short_index, &camera));

    // 3.1 亿音符也一样：判据只看小节数。
    let huge = index(310_979_003, 1920);
    assert!(!uses_overview(&huge, &camera));
    camera.zoom_x = 1920.0 / (30.0 * 1920.0);
    assert!(uses_overview(&huge, &camera));
}

#[test]
fn overview_threshold_follows_ppq() {
    let mut camera = camera(0.0);
    camera.zoom_y = 20.0;
    // PPQ 480 → 每小节 1920 tick；30 小节 = 57600 tick。
    camera.zoom_x = 1920.0 / (29.0 * 1920.0);
    assert!(!uses_overview(&index(1, 1920), &camera));
    camera.zoom_x = 1920.0 / (30.0 * 1920.0);
    assert!(uses_overview(&index(1, 1920), &camera));

    // PPQ 960 → 每小节 3840 tick；30 小节 = 115200 tick。
    camera.zoom_x = 1920.0 / (29.0 * 3840.0);
    assert!(!uses_overview(&index(1, 3840), &camera));
    camera.zoom_x = 1920.0 / (30.0 * 3840.0);
    assert!(uses_overview(&index(1, 3840), &camera));
}

#[test]
fn overview_never_switches_on_empty_document() {
    assert!(!uses_overview(&index(0, 1920), &camera(0.0001)));
}

#[test]
fn overview_rejects_invalid_zoom() {
    let index = index(1, 1920);
    let mut camera = camera(1.0);
    camera.zoom_x = f32::NAN;
    assert!(!uses_overview(&index, &camera));
    camera.zoom_x = 0.0;
    assert!(!uses_overview(&index, &camera));
    camera.zoom_x = 1.0;
    camera.zoom_y = f32::INFINITY;
    assert!(!uses_overview(&index, &camera));
}

#[test]
fn level_selection_covers_a_pixel() {
    let pyramid = DensityPyramid {
        levels: (0..LEVELS)
            .map(|index| DensityLevel {
                bin_ticks: BASE_BIN_TICKS << index,
                bins: 1,
                cells: vec![0; KEY_COUNT],
            })
            .collect(),
        max_counts: vec![1; LEVELS],
    };
    assert_eq!(pyramid.level_for_zoom(1.0), 0);
    assert!(pyramid.bin_ticks(pyramid.level_for_zoom(0.0001)) >= 1);
}

#[test]
fn heat_color_zero_is_background() {
    assert_eq!(heat_color(0, 100), COLOR_BG);
    assert_ne!(heat_color(100, 100), COLOR_BG);
}
