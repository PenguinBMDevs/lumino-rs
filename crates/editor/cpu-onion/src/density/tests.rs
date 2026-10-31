//! 密度金字塔与总览渲染单测。

use lumino_midi_model::{MidiDocument, NoteEvent};

use super::{
    DensityPyramid, OverviewScratch, heat_argb, overview_note_estimate, uses_overview,
    visible_measure_count,
};
use crate::{CanvasCamera, TrackIndex, color::NoteStyle};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 64;

fn camera(zoom_x: f32) -> CanvasCamera {
    CanvasCamera {
        width: WIDTH,
        height: HEIGHT,
        scroll_x: 0.0,
        scroll_y: 0.0,
        zoom_x,
        zoom_y: 20.0,
        key_count: 128,
    }
}

fn sample_document() -> MidiDocument {
    let mut document = MidiDocument::empty_with_tracks(2, 480);
    // 轨 1 是洋葱轨；tick 0 起、长 2000，key 125。
    document.insert_note(1, NoteEvent::new(0, 2000, 125, 100, 0));
    document
}

fn key_row(camera: &CanvasCamera, key: u16) -> u32 {
    (camera.key_to_y(key) + camera.zoom_y * 0.5) as u32
}

#[test]
fn build_bins_onsets_by_tick_and_key() {
    let pyramid = DensityPyramid::build(&sample_document(), 2, 128);
    assert_eq!(pyramid.key_count(), 128);
    assert_eq!(pyramid.total_ticks(), 2000);
    assert!(pyramid.levels.len() == super::LEVELS);
    // 基础层：tick 0 落在 bin 0，key 125 计数 1；其它格为 0。
    let bin_ticks = pyramid.bin_ticks(0);
    assert!(bin_ticks >= 64);
    let level = &pyramid.levels[0];
    assert_eq!(level.bin_ticks, bin_ticks);
    let cell = level.cells[125];
    assert_eq!(cell, 1, "tick 0 / key 125 的 onset 必须落在 bin 0");
    assert_eq!(level.cells[124], 0);
    // 上层由下层 2:1 合并，计数守恒。
    assert_eq!(pyramid.levels[1].cells[125], 1);
}

#[test]
fn level_for_zoom_keeps_bin_at_least_one_pixel() {
    let pyramid = DensityPyramid::build(&sample_document(), 1, 128);
    for zoom in [1.0f32, 0.5, 0.05, 0.001] {
        let level = pyramid.level_for_zoom(zoom);
        let bin = pyramid.bin_ticks(level);
        assert!(
            bin as f32 * zoom >= 1.0 || level == pyramid.levels.len() - 1,
            "zoom={zoom} 必须选到足够细的层（level={level}, bin={bin}）"
        );
    }
    assert_eq!(pyramid.level_for_zoom(1.0), 0);
    assert_eq!(pyramid.level_for_zoom(0.0), pyramid.levels.len() - 1);
}

#[test]
fn apply_note_matches_rebuild() {
    let original = sample_document();
    let mut edited = original.clone();
    edited.remove_note(1, 0);
    // 仍在既有 tick 范围内（长度变化不影响桶布局）。
    edited.insert_note(1, NoteEvent::new(640, 2000, 60, 100, 0));

    let mut incremental = DensityPyramid::build(&original, 1, 128);
    assert!(incremental.apply_note(0, 125, -1));
    assert!(incremental.apply_note(640, 60, 1));

    let rebuilt = DensityPyramid::build(&edited, 1, 128);
    assert_eq!(incremental.total_ticks(), rebuilt.total_ticks());
    for level in 0..incremental.levels.len() {
        assert_eq!(
            incremental.levels[level].cells, rebuilt.levels[level].cells,
            "第 {level} 层增量结果必须与整建一致"
        );
    }
}

#[test]
fn render_is_transparent_outside_density_and_aligned_with_keys() {
    let pyramid = DensityPyramid::build(&sample_document(), 1, 128);
    let camera = camera(0.01);
    let mut frame = vec![0u32; (WIDTH * HEIGHT) as usize];
    pyramid.render_overview(&mut frame, WIDTH, HEIGHT, &camera);

    let expected = heat_argb(1, 1);
    assert_ne!(expected, 0, "密度 > 0 必须是不透明色");
    let row = key_row(&camera, 125);
    assert_eq!(
        frame[(row * WIDTH) as usize],
        expected,
        "key 125 / tick 0 的格子必须是热力色"
    );
    // 相邻 key 行没有 onset → 透明（叠加层不得铺背景）。
    let empty_row = key_row(&camera, 126);
    assert_eq!(frame[(empty_row * WIDTH) as usize], 0);
    // 同一 key 行、远离 onset 的列 → 透明。
    assert_eq!(frame[(row * WIDTH + 32) as usize], 0);
}

#[test]
fn heat_lut_is_blue_for_low_density_and_white_at_peak() {
    assert_eq!(heat_argb(0, 100), 0, "零密度必须透明");
    let low = heat_argb(1, 1000);
    assert_eq!(low >> 24, 0xFF, "密度格必须不透明");
    let blue = low & 0xFF;
    let red = (low >> 16) & 0xFF;
    assert!(blue > red, "低密度必须是蓝色调（blue={blue}, red={red}）");
    assert_eq!(heat_argb(1000, 1000) & 0x00FF_FFFF, 0x00F5_FAFF);
}

#[test]
fn apply_note_outside_layout_requires_rebuild() {
    let pyramid = DensityPyramid::build(&sample_document(), 1, 128);
    let mut incremental = DensityPyramid::build(&sample_document(), 1, 128);
    // 远大于 2000 tick 的既有布局：必须拒绝，不能塞进末桶。
    assert!(
        !incremental.apply_note(10_000_000, 60, 1),
        "越过末 bin 的插入必须要求整建"
    );
    assert_eq!(
        incremental.total_ticks(),
        pyramid.total_ticks(),
        "拒绝后不得改动渲染右界"
    );
}

#[test]
fn overview_switches_at_thirty_visible_measures() {
    let document = sample_document();
    let index = TrackIndex::build(&document);
    let mut camera = camera(0.001);

    // 各缩放档的可见 tick 跨度 = measure_ticks × 小节数。
    camera.zoom_x = 64.0 / (29.0 * 1920.0);
    assert_eq!(visible_measure_count(&index, &camera), 29);
    assert!(!uses_overview(&index, &camera), "29 个小节必须 Exact");

    camera.zoom_x = 64.0 / (30.0 * 1920.0);
    assert_eq!(visible_measure_count(&index, &camera), 30);
    assert!(uses_overview(&index, &camera), "30 个小节切总览");

    camera.zoom_x = 64.0 / (31.0 * 1920.0);
    assert_eq!(visible_measure_count(&index, &camera), 31);
    assert!(uses_overview(&index, &camera), "31 个小节继续总览");
}

#[test]
fn overview_ignores_short_notes_and_key_row_height() {
    let mut document = MidiDocument::empty_with_tracks(2, 480);
    document.insert_note(1, NoteEvent::new(0, 2000, 60, 100, 0));
    document.insert_note(1, NoteEvent::new(2000, 2001, 61, 100, 0));
    let index = TrackIndex::build(&document);
    assert_eq!(index.short_note_ticks, 1);

    let mut camera = camera(0.001);
    // 29 个小节 + 1-tick 短音符 + 单键行高 0.5px：仍必须 Exact。
    camera.zoom_x = 64.0 / (29.0 * 1920.0);
    camera.zoom_y = 0.5;
    assert!(!uses_overview(&index, &camera));

    // 30 个小节才切总览，与短音符/键行无关。
    camera.zoom_x = 64.0 / (30.0 * 1920.0);
    assert!(uses_overview(&index, &camera));
}

#[test]
fn overview_threshold_follows_ppq() {
    let mut document = MidiDocument::empty_with_tracks(2, 960);
    document.insert_note(1, NoteEvent::new(0, 2000, 125, 100, 0));
    let index = TrackIndex::build(&document);

    // PPQ=960 → 每小节 3840 tick；30 小节 = 115200 tick。
    let mut camera = camera(0.001);
    camera.zoom_x = 64.0 / (29.0 * 3840.0);
    assert!(!uses_overview(&index, &camera), "29 小节保持 Exact");
    camera.zoom_x = 64.0 / (30.0 * 3840.0);
    assert!(uses_overview(&index, &camera), "30 小节切总览");
}

#[test]
fn visible_measure_count_includes_partial_edge_measures() {
    let document = sample_document();
    let index = TrackIndex::build(&document);
    let mut camera = camera(0.001);
    // 从第 0 小节中间开始，覆盖到第 30 小节中间：首尾两个部分小节都计入。
    camera.scroll_x = 1000.0;
    camera.zoom_x = 64.0 / (29.5 * 1920.0);
    assert!(visible_measure_count(&index, &camera) >= 30);
    assert!(uses_overview(&index, &camera));
}

#[test]
fn overview_never_switches_on_empty_document() {
    let document = MidiDocument::empty_with_tracks(2, 480);
    let index = TrackIndex::build(&document);
    let camera = camera(0.001);
    assert!(
        !uses_overview(&index, &camera),
        "空文档没有内容可密度化，不得切总览"
    );
}

#[test]
fn dirty_coverage_rebuild_matches_full_after_edit() {
    let mut before = MidiDocument::empty_with_tracks(3, 480);
    let removed = NoteEvent::new(0, 2000, 125, 100, 0);
    let added = NoteEvent::new(100, 300, 124, 100, 0);
    before.insert_note(0, removed);
    before.insert_note(1, NoteEvent::new(0, 2000, 125, 100, 1));
    before.insert_note(2, NoteEvent::new(0, 2000, 125, 100, 2));

    let mut pyramid = DensityPyramid::build(&before, 1, 128);
    let mut after = before.clone();
    after.remove_note(0, 0);
    after.insert_note(0, added);
    let index = TrackIndex::build(&after);
    assert!(pyramid.apply_note_range(removed.start_tick, removed.end_tick, 125, -1));
    assert!(pyramid.apply_note_range(added.start_tick, added.end_tick, 124, 1));
    assert!(!pyramid.coverage_valid());
    assert!(
        pyramid.rebuild_dirty_coverage(&after, &index),
        "有脏区时必须走局部重建"
    );
    assert!(pyramid.coverage_valid());

    let full = DensityPyramid::build(&after, 1, 128);
    let camera = camera(0.01);
    let style = NoteStyle::new(3);
    let mut patched = vec![0u32; (WIDTH * HEIGHT) as usize];
    let mut rebuilt = patched.clone();
    let mut scratch = OverviewScratch::default();
    pyramid.render_coverage_with_scratch(
        &mut patched,
        WIDTH,
        HEIGHT,
        &camera,
        &style,
        0,
        &[],
        &mut scratch,
    );
    full.render_coverage_with_scratch(
        &mut rebuilt,
        WIDTH,
        HEIGHT,
        &camera,
        &style,
        0,
        &[],
        &mut scratch,
    );
    assert_eq!(
        patched, rebuilt,
        "脏区局部重建必须与整建逐像素一致（含删除后下层轨道回退）"
    );
}

#[test]
fn coverage_render_uses_first_wins_and_second_candidate_for_excluded_track() {
    let mut document = MidiDocument::empty_with_tracks(3, 480);
    // 同一像素格内三条轨：0 最先生效；0 是当前轨时取 1；0/1 都排除时取 2。
    for track in 0..3 {
        document.insert_note(track, NoteEvent::new(0, 2000, 125, 100, 0));
    }
    let pyramid = DensityPyramid::build(&document, 1, 128);
    let camera = camera(0.01);
    let style = NoteStyle::new(3);
    let mut frame = vec![0u32; (WIDTH * HEIGHT) as usize];
    let mut scratch = OverviewScratch::default();
    let row = key_row(&camera, 125);

    pyramid.render_coverage_with_scratch(
        &mut frame,
        WIDTH,
        HEIGHT,
        &camera,
        &style,
        0,
        &[],
        &mut scratch,
    );
    assert_eq!(
        frame[(row * WIDTH) as usize],
        style.fill(1),
        "当前轨 0 必须由第二条候选轨 1 顶上"
    );

    pyramid.render_coverage_with_scratch(
        &mut frame,
        WIDTH,
        HEIGHT,
        &camera,
        &style,
        0,
        &[false, true, false],
        &mut scratch,
    );
    // 单元只保留前两条候选：当前轨 + 静音轨同时占住时留透明，绝不画错颜色。
    assert_eq!(
        frame[(row * WIDTH) as usize],
        0,
        "前两条候选都被排除时不得回退到未记录的第三条"
    );
}

#[test]
fn coverage_keeps_long_note_width_and_is_disabled_at_normal_zoom() {
    let mut document = MidiDocument::empty_with_tracks(1, 480);
    // 64 tick 宽：0.01 px/tick 下应覆盖 1 列；480 tick 宽应覆盖约 5 列。
    document.insert_note(0, NoteEvent::new(0, 64, 125, 100, 0));
    document.insert_note(0, NoteEvent::new(0, 480, 126, 100, 0));
    let pyramid = DensityPyramid::build(&document, 1, 128);
    let style = NoteStyle::new(1);
    let coverage_camera = camera(0.01);
    assert!(
        pyramid.can_render_coverage(&coverage_camera),
        "0.01 px/tick 必须启用覆盖光栅"
    );
    let mut frame = vec![0u32; (WIDTH * HEIGHT) as usize];
    let mut scratch = OverviewScratch::default();
    pyramid.render_coverage_with_scratch(
        &mut frame,
        WIDTH,
        HEIGHT,
        &coverage_camera,
        &style,
        usize::MAX,
        &[],
        &mut scratch,
    );
    let short_row = key_row(&coverage_camera, 125);
    let long_row = key_row(&coverage_camera, 126);
    let short_pixels = (0..WIDTH)
        .filter(|x| frame[(short_row * WIDTH + x) as usize] != 0)
        .count();
    let long_pixels = (0..WIDTH)
        .filter(|x| frame[(long_row * WIDTH + x) as usize] != 0)
        .count();
    assert_eq!(short_pixels, 1, "64 tick 在 0.01 px/tick 下应为 1 列");
    assert!(
        (4..=6).contains(&long_pixels),
        "480 tick 应保留约 5 列宽度（实际 {long_pixels}）"
    );

    let mut normal = camera(1.0);
    normal.width = WIDTH;
    assert!(
        !pyramid.can_render_coverage(&normal),
        "1 px/tick 必须回到 Exact，不得再用覆盖块"
    );
}

#[test]
fn coverage_invalidates_after_edit() {
    let mut document = sample_document();
    let mut pyramid = DensityPyramid::build(&document, 1, 128);
    assert!(pyramid.coverage_valid());
    assert!(pyramid.apply_note(0, 125, 1));
    assert!(!pyramid.coverage_valid(), "编辑后覆盖位图必须失效");
    document.insert_note(1, NoteEvent::new(640, 650, 60, 100, 0));
    let rebuilt = DensityPyramid::build(&document, 1, 128);
    assert!(rebuilt.coverage_valid());
}

#[test]
fn overview_note_estimate_remains_diagnostic_only() {
    let document = sample_document();
    let mut index = TrackIndex::build(&document);
    index.measure_counts = vec![200_000_000];
    let camera = camera(0.01);
    let estimate = overview_note_estimate(&index, &camera);
    assert!(
        estimate > 0,
        "估算仍应返回可见音符量（estimate={estimate}）"
    );

    index.measure_counts = vec![1];
    assert_eq!(overview_note_estimate(&index, &camera), 0);
}
