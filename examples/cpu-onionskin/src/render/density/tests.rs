use super::{render::heat_color, *};
use crate::{
    model::TrackIndex,
    render::{
        chrome::{COLOR_BG, COLOR_BLACK_ROW},
        color::NoteStyle,
    },
};
use lumino_midi_loader::{MidiDocument, NoteEvent};

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
        coverage: Vec::new(),
        total_ticks: 0,
        coverage_valid: true,
        coverage_dirty: None,
    };
    assert_eq!(pyramid.level_for_zoom(1.0), 0);
    assert!(pyramid.bin_ticks(pyramid.level_for_zoom(0.0001)) >= 1);
}

#[test]
fn heat_color_zero_is_background() {
    assert_eq!(heat_color(0, 100), COLOR_BG);
    assert_ne!(heat_color(100, 100), COLOR_BG);
}

#[test]
fn coverage_uses_track_color_and_main_track_blue() {
    let mut document = MidiDocument::empty_with_tracks(2, 480);
    document.insert_note(0, NoteEvent::new(0, 2000, 125, 100, 0));
    document.insert_note(1, NoteEvent::new(0, 2000, 125, 100, 0));
    let pyramid = DensityPyramid::build(&document, 1);
    let mut coverage_camera = camera(0.01);
    coverage_camera.scroll_y = 0.0;
    let style = NoteStyle::new(2);
    let mut frame = vec![0u32; 1920 * 1080];
    let mut scratch = OverviewScratch::default();
    let row = coverage_camera.key_to_y(125).round() as usize;

    pyramid.render_coverage(
        &mut frame,
        1920,
        1080,
        &coverage_camera,
        &style,
        0,
        &mut scratch,
    );
    assert_eq!(
        frame[row * 1920],
        style.main_fill,
        "当前轨必须显示主轨蓝，不能从高缩放画面消失"
    );

    pyramid.render_coverage(
        &mut frame,
        1920,
        1080,
        &coverage_camera,
        &style,
        9,
        &mut scratch,
    );
    assert_eq!(
        frame[row * 1920],
        style.fill(0, false),
        "非当前轨必须用真实轨道色（first-wins 仍是轨 0）"
    );

    // 当前轨 1 是第二条候选：必须优先显示主轨蓝，不能被轨 0 遮掉。
    pyramid.render_coverage(
        &mut frame,
        1920,
        1080,
        &coverage_camera,
        &style,
        1,
        &mut scratch,
    );
    assert_eq!(
        frame[row * 1920],
        style.main_fill,
        "当前轨即使不是 first-wins 第一条，也必须优先显示"
    );
}

#[test]
fn coverage_keeps_note_width_and_disables_at_normal_zoom() {
    let mut document = MidiDocument::empty_with_tracks(1, 480);
    document.insert_note(0, NoteEvent::new(0, 64, 125, 100, 0));
    document.insert_note(0, NoteEvent::new(0, 480, 126, 100, 0));
    let pyramid = DensityPyramid::build(&document, 1);
    let style = NoteStyle::new(1);
    let mut coverage_camera = camera(0.01);
    coverage_camera.scroll_y = 0.0;
    assert!(
        pyramid.can_render_coverage(&coverage_camera),
        "0.01 px/tick 必须启用覆盖光栅"
    );
    let mut frame = vec![0u32; 1920 * 1080];
    let mut scratch = OverviewScratch::default();
    pyramid.render_coverage(
        &mut frame,
        1920,
        1080,
        &coverage_camera,
        &style,
        0,
        &mut scratch,
    );
    let short_row = coverage_camera.key_to_y(125).round() as usize;
    let long_row = coverage_camera.key_to_y(126).round() as usize;
    let short_pixels = (0..1920)
        .filter(|x| frame[short_row * 1920 + x] != COLOR_BG)
        .count();
    let long_pixels = (0..1920)
        .filter(|x| frame[long_row * 1920 + x] != COLOR_BLACK_ROW)
        .count();
    assert_eq!(short_pixels, 1, "64 tick 在 0.01 px/tick 下应为 1 列");
    assert!(
        (4..=6).contains(&long_pixels),
        "480 tick 应保留约 5 列宽度（实际 {long_pixels}）"
    );

    let normal = camera(1.0);
    assert!(
        !pyramid.can_render_coverage(&normal),
        "1 px/tick 必须回到 Exact"
    );
}

#[test]
fn dirty_coverage_rebuild_matches_full_after_edit() {
    let mut before = MidiDocument::empty_with_tracks(2, 480);
    let removed = NoteEvent::new(0, 480, 125, 100, 0);
    let added = NoteEvent::new(100, 300, 126, 100, 0);
    before.insert_note(0, removed);
    before.insert_note(1, NoteEvent::new(0, 480, 125, 100, 1));
    let mut pyramid = DensityPyramid::build(&before, 1);

    let mut after = before.clone();
    after.remove_note(0, 0);
    after.insert_note(0, added);
    let index = TrackIndex::build(&after);
    pyramid.apply_note_range(removed.start_tick, removed.end_tick, 125, -1);
    pyramid.apply_note_range(added.start_tick, added.end_tick, 126, 1);
    assert!(!pyramid.coverage_valid());
    assert!(
        pyramid.rebuild_dirty_coverage(&after, &index),
        "有脏区时必须局部重建"
    );
    assert!(pyramid.coverage_valid());

    let full = DensityPyramid::build(&after, 1);
    let mut camera = camera(0.01);
    camera.scroll_y = 0.0;
    let style = NoteStyle::new(2);
    let mut patched = vec![0u32; 1920 * 1080];
    let mut rebuilt = patched.clone();
    let mut scratch = OverviewScratch::default();
    pyramid.render_coverage(&mut patched, 1920, 1080, &camera, &style, 0, &mut scratch);
    full.render_coverage(&mut rebuilt, 1920, 1080, &camera, &style, 0, &mut scratch);
    assert_eq!(patched, rebuilt, "脏区局部重建必须与整建逐像素一致");
}

#[test]
fn out_of_layout_insert_grows_and_matches_full() {
    let mut before = MidiDocument::empty_with_tracks(1, 480);
    before.insert_note(0, NoteEvent::new(0, 100, 125, 100, 0));
    let mut pyramid = DensityPyramid::build(&before, 1);

    let mut after = before.clone();
    let note = NoteEvent::new(3000, 3200, 60, 100, 0);
    after.insert_note(0, note);
    assert!(
        pyramid.apply_note_range(note.start_tick, note.end_tick, note.key, 1),
        "越过既有右界只应扩容，不得要求整建"
    );
    assert!(pyramid.total_ticks() >= note.end_tick);
    let index = TrackIndex::build(&after);
    assert!(pyramid.rebuild_dirty_coverage(&after, &index));

    let full = DensityPyramid::build(&after, 1);
    let mut camera = camera(0.01);
    camera.scroll_y = 0.0;
    let style = NoteStyle::new(1);
    let mut patched = vec![0u32; 1920 * 1080];
    let mut rebuilt = patched.clone();
    let mut scratch = OverviewScratch::default();
    pyramid.render_coverage(&mut patched, 1920, 1080, &camera, &style, 0, &mut scratch);
    full.render_coverage(&mut rebuilt, 1920, 1080, &camera, &style, 0, &mut scratch);
    assert_eq!(patched, rebuilt, "扩容后必须与整建逐像素一致");
}

#[test]
fn range_rebuild_matches_full_after_large_edit() {
    let mut before = MidiDocument::empty_with_tracks(1, 480);
    for index in 0..100u32 {
        before.insert_note(0, NoteEvent::new(index * 10, index * 10 + 5, 60, 100, 0));
    }
    let mut pyramid = DensityPyramid::build(&before, 1);
    let mut after = before.clone();
    after.remove_note_ranges(0, &[(0, 100)]);
    let index = TrackIndex::build(&after);
    assert!(pyramid.rebuild_dirty_ranges(&after, &index, &[(0, 1000)]));

    let full = DensityPyramid::build(&after, 1);
    let mut camera = camera(0.01);
    camera.scroll_y = 0.0;
    let style = NoteStyle::new(1);
    let mut patched = vec![0u32; 1920 * 1080];
    let mut rebuilt = patched.clone();
    let mut scratch = OverviewScratch::default();
    pyramid.render_coverage(&mut patched, 1920, 1080, &camera, &style, 0, &mut scratch);
    full.render_coverage(&mut rebuilt, 1920, 1080, &camera, &style, 0, &mut scratch);
    assert_eq!(patched, rebuilt, "区间重建必须与整建逐像素一致");
}

#[test]
fn coverage_invalidates_after_edit() {
    let mut document = MidiDocument::empty_with_tracks(1, 480);
    document.insert_note(0, NoteEvent::new(0, 100, 125, 100, 0));
    let mut pyramid = DensityPyramid::build(&document, 1);
    assert!(pyramid.coverage_valid());
    pyramid.apply_note_range(0, 100, 125, 1);
    assert!(!pyramid.coverage_valid(), "编辑后覆盖层必须失效");
    document.insert_note(0, NoteEvent::new(200, 300, 125, 100, 0));
    let rebuilt = DensityPyramid::build(&document, 1);
    assert!(rebuilt.coverage_valid());
}
