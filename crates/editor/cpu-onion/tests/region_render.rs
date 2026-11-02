//! 区域重绘验收：`render_region` 与 `render_full` **逐像素**等价。
//!
//! 判据统一是「整帧」而不是「区域」：先把上一版文档整帧渲染，再对**变化矩形**
//! 做一次区域重绘，结果必须与直接整帧渲染新文档完全相同——区域外若有任何旧像素
//! 残留、区域内若少画/多画一个像素，都会立刻失败。

use std::sync::Arc;

use lumino_cpu_onion::{CanvasCamera, CpuOnionRect, CpuOnionRenderer, TrackIndex};
use lumino_midi_model::{MidiDocument, NoteEvent};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 240;

/// 轨 0 = 当前轨（CPU 跳过）、轨 1 = 洋葱皮轨、轨 2 = 静音轨。
const MUTED: [bool; 3] = [false, false, true];

fn camera() -> CanvasCamera {
    CanvasCamera {
        width: WIDTH,
        height: HEIGHT,
        scroll_x: 0.0,
        scroll_y: 0.0,
        zoom_x: 1.0,
        zoom_y: 20.0,
        key_count: 128,
    }
}

/// 基准文档：短音符（轨 0/1/2 各一）+ 一条跨 band 的洋葱皮长音符（700 tick）。
fn document() -> MidiDocument {
    let mut document = MidiDocument::empty_with_tracks(3, 480);
    document.insert_note(0, NoteEvent::new(10, 110, 120, 100, 0));
    document.insert_note(1, NoteEvent::new(10, 110, 121, 100, 0));
    document.insert_note(2, NoteEvent::new(10, 110, 122, 100, 0));
    // 700 tick > LOOKBACK_TICKS(512) → 走长音符通道；横跨多个 band。
    document.insert_note(1, NoteEvent::new(200, 900, 118, 100, 0));
    document
}

/// 音符在帧内的像素矩形（向四周扩 `margin` 像素），夹到画布内。
fn note_rect(camera: &CanvasCamera, note: &NoteEvent, margin: i32) -> CpuOnionRect {
    let sx = note.start_tick as f32 * camera.zoom_x - camera.scroll_x;
    let sw = note.length() as f32 * camera.zoom_x;
    let sy = camera.key_to_y(note.key as u16);
    let sh = camera.zoom_y;
    let x0 = ((sx.floor() as i32) - margin).max(0) as u32;
    let x1 = (((sx + sw).ceil() as i32) + margin)
        .min(camera.width as i32)
        .max(0) as u32;
    let y0 = ((sy.floor() as i32) - margin).max(0) as u32;
    let y1 = (((sy + sh).ceil() as i32) + margin)
        .min(camera.height as i32)
        .max(0) as u32;
    CpuOnionRect::new(x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0))
}

fn merged_rect(camera: &CanvasCamera, notes: &[NoteEvent], margin: i32) -> CpuOnionRect {
    notes
        .iter()
        .fold(CpuOnionRect::new(0, 0, 0, 0), |acc, note| {
            acc.union(note_rect(camera, note, margin))
        })
}

/// 直接整帧渲染 `document` 的像素。
fn full_pixels(document: &MidiDocument, camera: &CanvasCamera) -> Arc<Vec<u8>> {
    let document = Arc::new(document.clone());
    let index = Arc::new(TrackIndex::build(&document));
    let mut renderer = CpuOnionRenderer::new(4, 3);
    renderer.resize(WIDTH, HEIGHT);
    renderer
        .render_full(document, index, camera, 0, &MUTED)
        .pixels
        .clone()
}

/// 先整帧渲染 `before`，再对 `rect` 区域重绘 `after`，返回重绘后的整帧像素。
fn patched_pixels(
    before: &MidiDocument,
    after: &MidiDocument,
    camera: &CanvasCamera,
    rect: CpuOnionRect,
) -> Arc<Vec<u8>> {
    let before = Arc::new(before.clone());
    let before_index = Arc::new(TrackIndex::build(&before));
    let mut renderer = CpuOnionRenderer::new(4, 3);
    renderer.resize(WIDTH, HEIGHT);
    renderer.render_full(before, before_index, camera, 0, &MUTED);

    let after = Arc::new(after.clone());
    let after_index = Arc::new(TrackIndex::build(&after));
    renderer
        .render_region(after, after_index, camera, 0, &MUTED, rect)
        .pixels
        .clone()
}

/// 断言「区域重绘 == 整帧重绘」，且变化确实发生（防止测试空转）。
fn assert_region_matches_full(before: &MidiDocument, after: &MidiDocument, rect: CpuOnionRect) {
    let camera = camera();
    let expected = full_pixels(after, &camera);
    let baseline = full_pixels(before, &camera);
    assert_ne!(
        baseline, expected,
        "用例设计错误：before/after 整帧像素必须不同，否则测不出脏区清除"
    );
    let got = patched_pixels(before, after, &camera, rect);
    assert_eq!(
        got.len(),
        expected.len(),
        "区域重绘不得改变帧尺寸/像素缓冲长度"
    );
    assert_eq!(
        got, expected,
        "对 rect={rect:?} 的区域重绘必须与整帧重绘逐像素一致（区域外有旧像素残留或区域内少画都会失败）"
    );
}

/// 区域覆盖整帧时与 `render_full` 逐像素一致（含旧像素清空）。
#[test]
fn full_frame_region_matches_full_render() {
    let camera = camera();
    let before = document();
    let clean = MidiDocument::empty_with_tracks(3, 480);
    let rect = CpuOnionRect::new(0, 0, WIDTH, HEIGHT);
    let expected = full_pixels(&before, &camera);
    // 先用「干净文档」画一帧（全透明），再整帧区域重绘 → 必须等于直接整帧渲染
    let got = patched_pixels(&clean, &before, &camera, rect);
    assert_eq!(got, expected);
}

/// 插入：区域只覆盖新音符，其余旧像素必须保留。
#[test]
fn insert_region_matches_full_render() {
    let camera = camera();
    let before = document();
    let mut after = document();
    // key 124 → y = 60，落在 240 高的画布内（key < 116 会被画布裁掉）
    let added = NoteEvent::new(300, 420, 124, 100, 0);
    after.insert_note(1, added);

    let rect = note_rect(&camera, &added, 4);
    assert_region_matches_full(&before, &after, rect);
}

/// 删除：区域必须把旧音符像素**清回透明**（脏区清除的核心用例）。
#[test]
fn remove_region_clears_old_pixels() {
    let camera = camera();
    let before = document();
    let removed = NoteEvent::new(10, 110, 121, 100, 0);
    let mut after = document();
    after.remove_note(1, 0);

    let rect = note_rect(&camera, &removed, 4);
    assert_region_matches_full(&before, &after, rect);

    // 直接验证：新帧里该音符所在行必须回到全透明
    let row = (camera.key_to_y(removed.key as u16) + camera.zoom_y * 0.5) as u32;
    let got = patched_pixels(&before, &after, &camera, rect);
    let offset = ((row as usize) * WIDTH as usize + 60) * 4;
    assert_eq!(
        &got[offset..offset + 4],
        &[0, 0, 0, 0],
        "被删除音符的旧像素必须被区域重绘清空"
    );
}

/// 更新（移动）：旧位置与新位置同时落在 union 矩形内。
#[test]
fn update_region_covers_old_and_new_position() {
    let camera = camera();
    let before = document();
    let old = NoteEvent::new(10, 110, 121, 100, 0);
    let new = NoteEvent::new(410, 560, 116, 100, 0);
    let mut after = document();
    after.remove_note(1, 0);
    after.insert_note(1, new);

    let rect = merged_rect(&camera, &[old, new], 4);
    assert_region_matches_full(&before, &after, rect);
}

/// 长音符跨区域：只重画窄条时，长音符在区域外的部分必须保持原样。
#[test]
fn long_note_spanning_region_is_clipped_not_lost() {
    let camera = camera();
    let document = document();
    let index = Arc::new(TrackIndex::build(&document));
    let expected = full_pixels(&document, &camera);

    let mut renderer = CpuOnionRenderer::new(4, 3);
    renderer.resize(WIDTH, HEIGHT);
    let document = Arc::new(document);
    renderer.render_full(
        Arc::clone(&document),
        Arc::clone(&index),
        &camera,
        0,
        &MUTED,
    );
    // 窄条只覆盖长音符中段（200..900 tick 的中部），band 宽度 37 不能被 4 整除
    let rect = CpuOnionRect::new(300, 172, 37, 34);
    let got = renderer
        .render_region(document, index, &camera, 0, &MUTED, rect)
        .pixels
        .clone();
    assert_eq!(
        got, expected,
        "同一文档再做一次窄条区域重绘必须完全幂等（长音符被裁剪而非丢失）"
    );
}

/// 当前轨 / 静音轨的编辑：CPU 帧两边都透明，区域重绘结果不变。
#[test]
fn current_and_muted_track_edits_leave_frame_unchanged() {
    let camera = camera();
    let before = document();
    let mut after = document();
    // 当前轨（0）与静音轨（2）各加一个音符
    after.insert_note(0, NoteEvent::new(300, 420, 110, 100, 0));
    after.insert_note(2, NoteEvent::new(300, 420, 109, 100, 0));

    let expected = full_pixels(&before, &camera);
    let got = patched_pixels(
        &before,
        &after,
        &camera,
        CpuOnionRect::new(0, 0, WIDTH, HEIGHT),
    );
    assert_eq!(got, expected, "当前轨/静音轨由 GPU 负责，CPU 帧不得变化");
}

/// 边框裁剪：区域只覆盖音符的内侧一段，边框像素与整帧渲染一致。
#[test]
fn region_cutting_through_note_keeps_pixels_identical() {
    let camera = camera();
    let document = document();
    let expected = full_pixels(&document, &camera);

    let mut renderer = CpuOnionRenderer::new(4, 3);
    renderer.resize(WIDTH, HEIGHT);
    let document = Arc::new(document);
    let index = Arc::new(TrackIndex::build(&document));
    renderer.render_full(
        Arc::clone(&document),
        Arc::clone(&index),
        &camera,
        0,
        &MUTED,
    );
    // 短音符 10..110 x、key 121 行（y 120..140）的**内侧**切片
    let rect = CpuOnionRect::new(40, 125, 20, 6);
    let got = renderer
        .render_region(document, index, &camera, 0, &MUTED, rect)
        .pixels
        .clone();
    assert_eq!(got, expected, "切穿音符内部的区域重绘必须与整帧一致");
}

/// 完全越界 / 空矩形：不改动任何像素。
#[test]
fn out_of_bounds_region_is_a_noop() {
    let camera = camera();
    let document = document();
    let expected = full_pixels(&document, &camera);

    let mut renderer = CpuOnionRenderer::new(4, 3);
    renderer.resize(WIDTH, HEIGHT);
    let document = Arc::new(document);
    let index = Arc::new(TrackIndex::build(&document));
    renderer.render_full(
        Arc::clone(&document),
        Arc::clone(&index),
        &camera,
        0,
        &MUTED,
    );
    for rect in [
        CpuOnionRect::new(WIDTH, 0, 10, 10),
        CpuOnionRect::new(0, HEIGHT, 10, 10),
        CpuOnionRect::new(10, 10, 0, 0),
        CpuOnionRect::new(10, 10, 0, 10),
    ] {
        let got = renderer
            .render_region(
                Arc::clone(&document),
                Arc::clone(&index),
                &camera,
                0,
                &MUTED,
                rect,
            )
            .pixels
            .clone();
        assert_eq!(got, expected, "rect={rect:?} 越界/空矩形不得改动像素");
    }
}

/// 区域跨画布边界：裁剪后仍与整帧一致。
#[test]
fn region_clipped_at_frame_edge_matches_full() {
    let camera = camera();
    let before = document();
    let old = NoteEvent::new(200, 900, 118, 100, 0);
    let new = NoteEvent::new(500, 1200, 118, 100, 0);
    let mut after = document();
    after.remove_note(1, 1);
    after.insert_note(1, new);

    // 变化像素横跨 [200, 640)：矩形必须覆盖新旧两条长音符的可见部分
    let rect = merged_rect(&camera, &[old, new], 4);
    assert!(rect.x + rect.width >= WIDTH, "用例应覆盖到画布右边界");
    assert_region_matches_full(&before, &after, rect);
}
