//! 密度金字塔与总览渲染单测。

use lumino_midi_model::{MidiDocument, NoteEvent};

use super::{
    DensityPyramid, OVERVIEW_MIN_NOTE_PX, SHORT_NOTE_MIN_PX, heat_argb, overview_note_estimate,
    uses_overview,
};
use crate::{CanvasCamera, TrackIndex};

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
fn overview_switches_when_short_notes_are_subpixel() {
    let mut document = MidiDocument::empty_with_tracks(2, 480);
    document.insert_note(1, NoteEvent::new(0, 2000, 60, 100, 0));
    // 1-tick 短音符进入第 10 百分位。
    document.insert_note(1, NoteEvent::new(2000, 2001, 61, 100, 0));
    let index = TrackIndex::build(&document);
    assert_eq!(index.short_note_ticks, 1);

    let mut camera = camera(0.5);
    assert!(
        uses_overview(&index, &camera),
        "短音符 0.5px 时逐音符只会退化成整曲扫描，必须切总览"
    );
    camera.zoom_x = 1.0;
    assert!(uses_overview(&index, &camera), "短音符 1px 仍低于 2px 阈值");
    camera.zoom_x = SHORT_NOTE_MIN_PX;
    assert!(!uses_overview(&index, &camera), "短音符 2px 仍保持 Exact");
}

#[test]
fn overview_switches_by_zoom_not_note_count() {
    let document = sample_document();
    let mut index = TrackIndex::build(&document);
    // 1 个音符：数量极少，但 1/16 音符只有 1.2px（< 2px）→ 必须切总览。
    let mut camera = camera(0.01);
    assert!(
        uses_overview(&index, &camera),
        "缩放阈值触发不应依赖同屏音符数量"
    );

    // 反向证明：把聚合计数伪造到 2 亿，正常缩放也不得因「数量多」切档。
    index.measure_counts = vec![200_000_000];
    camera.zoom_x = 1.0;
    assert!(
        !uses_overview(&index, &camera),
        "正常缩放下不得再按同屏音符数量切档"
    );

    // 计数恢复为 1：极限缩放仍应触发。
    index.measure_counts = vec![1];
    camera.zoom_x = 0.005;
    assert!(uses_overview(&index, &camera));
}

#[test]
fn overview_switches_by_key_row_height() {
    let document = sample_document();
    let index = TrackIndex::build(&document);
    let mut camera = camera(1.0);
    assert!(!uses_overview(&index, &camera), "横向正常时不应切档");
    camera.zoom_y = 1.0;
    assert!(
        uses_overview(&index, &camera),
        "单键行高 < 2px 时 Exact 已退化成线条，必须切总览"
    );
}

#[test]
fn overview_zoom_threshold_follows_ppq() {
    let mut document = MidiDocument::empty_with_tracks(2, 960);
    document.insert_note(1, NoteEvent::new(0, 2000, 125, 100, 0));
    let index = TrackIndex::build(&document);

    // PPQ=960：1/16 = 240 tick；0.01px/tick → 2.4px（Exact），0.008 → 1.92px（总览）。
    let mut camera = camera(0.01);
    assert!(
        !uses_overview(&index, &camera),
        "960 PPQ 下 0.01 仍是 2.4px"
    );
    camera.zoom_x = 0.008;
    assert!(uses_overview(&index, &camera), "960 PPQ 下 0.008 已 < 2px");
}

#[test]
fn overview_threshold_is_two_physical_pixels() {
    let document = sample_document();
    let index = TrackIndex::build(&document);
    let mut camera = camera(1.0);

    // 恰好 2px：仍是 Exact（判定为严格小于）。
    camera.zoom_x = OVERVIEW_MIN_NOTE_PX / 120.0;
    assert!(!uses_overview(&index, &camera), "2px 边界应保持 Exact");
    camera.zoom_x *= 0.999;
    assert!(uses_overview(&index, &camera), "略低于 2px 应切总览");

    camera.zoom_x = 1.0;
    camera.zoom_y = OVERVIEW_MIN_NOTE_PX;
    assert!(!uses_overview(&index, &camera), "键行 2px 边界应保持 Exact");
    camera.zoom_y *= 0.999;
    assert!(uses_overview(&index, &camera), "键行略低于 2px 应切总览");
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
