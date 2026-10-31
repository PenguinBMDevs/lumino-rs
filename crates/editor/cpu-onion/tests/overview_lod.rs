//! 极限缩放 LOD 回归：彩色覆盖光栅必须替代逐音符扫描/蓝色热力图。
//!
//! 覆盖光栅保留真实轨道色与音符宽度；当前轨/静音轨通过单元内第一、第二候选
//! 过滤。普通缩放仍必须逐音符 Exact。

use std::sync::Arc;

use lumino_cpu_onion::{
    CanvasCamera, CpuOnionRenderer, DensityPyramid, TrackIndex, color::NoteStyle,
};
use lumino_midi_model::{MidiDocument, NoteEvent};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 240;

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

fn document() -> Arc<MidiDocument> {
    let mut document = MidiDocument::empty_with_tracks(2, 480);
    document.insert_note(1, NoteEvent::new(0, 2000, 120, 100, 0));
    Arc::new(document)
}

fn row_center(camera: &CanvasCamera, key: u16) -> u32 {
    (camera.key_to_y(key) + camera.zoom_y * 0.5) as u32
}

#[test]
fn extreme_zoom_renders_colored_coverage_instead_of_notes() {
    let document = document();
    let index = Arc::new(TrackIndex::build(&document));
    let density = DensityPyramid::build(document.as_ref(), 2, 128);
    let camera = camera(0.01);
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

    let expected = NoteStyle::new(2).fill_rgba(1);
    assert_eq!(
        density_pixel, expected,
        "极限缩放必须画真实轨道色的覆盖块（实际 {density_pixel:?}）"
    );
    assert_eq!(density_pixel[3], 255, "覆盖块必须不透明");
    assert_eq!(
        empty_row_pixel,
        [0, 0, 0, 0],
        "没有 onset 的 key 行必须保持透明（叠加层不得铺背景）"
    );

    // 同一帧尺寸下走 Exact：x=0 是音符左边框，颜色必须与覆盖块不同。
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
    assert_ne!(
        exact_pixel, density_pixel,
        "覆盖 LOD 必须真正替代逐音符外观"
    );
}

#[test]
fn overview_gate_stays_exact_at_normal_zoom() {
    let document = document();
    let index = Arc::new(TrackIndex::build(&document));
    let density = DensityPyramid::build(document.as_ref(), 1, 128);
    // 正常缩放：1/16 音符 120px、键行 20px，即使传了密度也不得切档。
    let camera = camera(1.0);
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
        "缩放阈值以上必须保持 Exact，不得误切总览"
    );
}
