//! 极限缩放总览 LOD 回归：蓝色密度层必须替代逐音符光栅。
//!
//! 复现缺陷：CPU 洋葱皮只抽取了示例的 Exact 路径，`examples/cpu-onionskin`
//! 的 Overview 密度金字塔没有跟着进生产；极限缩放下仍逐音符扫描，蓝色简化层
//! 不出现（大工程还会卡死 UI）。

use std::sync::Arc;

use lumino_cpu_onion::{
    CanvasCamera, CpuOnionRenderer, DensityPyramid, OVERVIEW_NOTE_THRESHOLD, TrackIndex,
    color::NoteStyle, density::heat_argb,
};
use lumino_midi_model::{MidiDocument, NoteEvent};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 240;

fn camera() -> CanvasCamera {
    CanvasCamera {
        width: WIDTH,
        height: HEIGHT,
        scroll_x: 0.0,
        scroll_y: 0.0,
        // 极限缩小：20 tick/px 的音符只剩 0.2px 宽。
        zoom_x: 0.01,
        zoom_y: 20.0,
        key_count: 128,
    }
}

fn document() -> Arc<MidiDocument> {
    let mut document = MidiDocument::empty_with_tracks(2, 480);
    document.insert_note(1, NoteEvent::new(0, 2000, 120, 100, 0));
    Arc::new(document)
}

/// 伪造一个「一屏 2000 万可见音符」的工程：只改小节聚合计数，不造 2000 万音符。
fn crowded_index(document: &MidiDocument) -> Arc<TrackIndex> {
    let mut index = TrackIndex::build(document);
    index.measure_counts = vec![200_000_000];
    Arc::new(index)
}

fn row_center(camera: &CanvasCamera, key: u16) -> u32 {
    (camera.key_to_y(key) + camera.zoom_y * 0.5) as u32
}

#[test]
fn extreme_zoom_renders_blue_density_instead_of_notes() {
    let document = document();
    let index = crowded_index(&document);
    let density = DensityPyramid::build(document.as_ref(), 2, 128);
    let camera = camera();
    let mut renderer = CpuOnionRenderer::new(2, 2);
    renderer.resize(WIDTH, HEIGHT);

    let frame = renderer.render_full_with_density(
        Arc::clone(&document),
        Arc::clone(&index),
        &camera,
        0,
        &[false, false],
        Some(&density),
    );
    let row = row_center(&camera, 120);
    let density_pixel = frame.pixel(0, row).expect("帧内像素");
    let empty_row_pixel = frame.pixel(0, row_center(&camera, 124)).expect("帧内像素");
    let visible = renderer.visible_notes();

    let expected = lumino_cpu_onion::color::argb_to_rgba(heat_argb(1, 1));
    assert_eq!(
        density_pixel, expected,
        "极限缩放必须画蓝色密度层（实际 {density_pixel:?}）"
    );
    assert_eq!(density_pixel[3], 255, "密度格必须不透明");
    assert_eq!(
        empty_row_pixel,
        [0, 0, 0, 0],
        "没有 onset 的 key 行必须保持透明（叠加层不得铺背景）"
    );
    assert!(
        visible > OVERVIEW_NOTE_THRESHOLD,
        "总览判定必须记录可见音符估算（visible={visible}）"
    );

    // 同一帧尺寸下走 Exact：x=0 是音符左边框，颜色必须与热力色不同。
    let exact_frame = renderer.render_full(
        Arc::clone(&document),
        Arc::clone(&index),
        &camera,
        0,
        &[false, false],
    );
    let exact_pixel = exact_frame.pixel(0, row).expect("帧内像素");
    let border = NoteStyle::new(2).border_rgba(1);
    assert_eq!(exact_pixel, border, "Exact 路径必须仍是逐音符描边");
    assert_ne!(exact_pixel, density_pixel, "总览 LOD 必须真正替换外观");
}

#[test]
fn overview_gate_stays_exact_for_small_documents() {
    let document = document();
    // 正常索引：可见音符远低于 6M，即使传了密度也不得切换档位。
    let index = Arc::new(TrackIndex::build(&document));
    let density = DensityPyramid::build(document.as_ref(), 1, 128);
    let camera = camera();
    let mut renderer = CpuOnionRenderer::new(1, 2);
    renderer.resize(WIDTH, HEIGHT);

    let frame = renderer.render_full_with_density(
        document,
        index,
        &camera,
        0,
        &[false, false],
        Some(&density),
    );
    let row = row_center(&camera, 120);
    assert_eq!(
        frame.pixel(0, row),
        Some(NoteStyle::new(2).border_rgba(1)),
        "阈值以下必须保持 Exact，不得误切总览"
    );
}
