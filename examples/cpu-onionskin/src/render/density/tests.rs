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
    camera
}

#[test]
fn overview_switches_only_when_short_notes_are_subpixel() {
    // 黑乐谱短音符 1 tick：只有真正亚像素（<1px）才切总览；1px/tick 必须
    // 保留原始音符，否则高缩放会被误显示成热度图。
    let mut index = index(1, 1920);
    index.short_note_ticks = 1;
    let mut camera = camera(0.5);
    camera.zoom_y = 20.0;
    assert!(uses_overview(&index, &camera), "短音符 0.5px 应切总览");
    camera.zoom_x = SHORT_NOTE_MIN_PX;
    assert!(!uses_overview(&index, &camera), "短音符 1px 必须保持 Exact");
    camera.zoom_x *= 0.999;
    assert!(uses_overview(&index, &camera), "略低于 1px 才切总览");
}

#[test]
fn overview_switches_by_zoom_not_note_count() {
    // 1 个音符也能在极限缩放下切总览。
    let tiny = index(1, 1920);
    assert!(uses_overview(&tiny, &camera(0.001)));
    // 即使在旧口径下"可见量巨大"，正常缩放也必须 Exact。
    let huge = index(310_979_003, 1920);
    assert!(!uses_overview(&huge, &camera(1.0)));
}

#[test]
fn overview_horizontal_boundary_is_two_physical_pixels() {
    // PPQ 480 → 1/16 = 120 tick；2px 边界 = 1/60 px/tick。
    let index = index(1, 1920);
    let mut camera = camera(0.0);
    camera.zoom_y = 20.0;
    camera.zoom_x = OVERVIEW_MIN_NOTE_PX / 120.0;
    assert!(!uses_overview(&index, &camera), "2px 边界应保持 Exact");
    camera.zoom_x = OVERVIEW_MIN_NOTE_PX / 120.0 - 0.000_01;
    assert!(uses_overview(&index, &camera), "略低于 2px 应切总览");
}

#[test]
fn overview_vertical_boundary_is_two_physical_pixels() {
    let index = index(1, 1920);
    let mut camera = camera(1.0);
    camera.zoom_y = OVERVIEW_MIN_NOTE_PX;
    assert!(!uses_overview(&index, &camera), "键行 2px 边界应保持 Exact");
    camera.zoom_y = OVERVIEW_MIN_NOTE_PX - 0.01;
    assert!(uses_overview(&index, &camera), "键行略低于 2px 应切总览");
}

#[test]
fn overview_threshold_follows_ppq() {
    let mut camera = camera(0.0);
    camera.zoom_y = 20.0;
    camera.zoom_x = 0.01;
    // 480 PPQ：1/16=120 tick，0.01×120=1.2px → 总览。
    assert!(uses_overview(&index(1, 1920), &camera));
    // 960 PPQ：1/16=240 tick，0.01×240=2.4px → Exact。
    assert!(!uses_overview(&index(1, 3840), &camera));
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
