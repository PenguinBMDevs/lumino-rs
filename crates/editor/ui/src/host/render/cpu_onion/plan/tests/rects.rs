//! 脏矩形换算验收（`plan/tests.rs` 的子模块，保持每个测试文件 < 400 行）。

use lumino_midi_model::NoteEvent;

use super::super::{CpuOnionRedraw, plan_update};
use super::{WIDTH, base, camera, change, dirty_with, tracks};

/// 完全在画布外的变更 → 不重绘（但仍更新索引）。
#[test]
fn offscreen_change_skips_redraw() {
    let muted = [false, false, true];
    let changed = tracks(&[1]);
    let mut moved = camera();
    moved.scroll_x = 10_000.0;
    let dirty = dirty_with(vec![change(
        1,
        None,
        Some(NoteEvent::new(0, 100, 121, 100, 0)),
    )]);
    let plan = plan_update(base(&moved, &muted, Some(&dirty), Some(&changed)));
    assert_eq!(plan.redraw, None);
}

/// 越界脏区被裁到画布内（不得产生越界矩形）。
#[test]
fn dirty_rect_is_clipped_to_frame() {
    let camera = camera();
    let muted = [false, false, true];
    let changed = tracks(&[1]);
    let note = NoteEvent::new(600, 900, 120, 100, 0);
    let dirty = dirty_with(vec![change(1, None, Some(note))]);
    let plan = plan_update(base(&camera, &muted, Some(&dirty), Some(&changed)));
    let Some(CpuOnionRedraw::Region(rect)) = plan.redraw else {
        panic!("应产生区域重绘，实际 {:?}", plan.redraw);
    };
    assert_eq!(rect.x + rect.width, WIDTH, "x 必须裁到画布右边界");
    assert!(rect.area() > 0);
}

/// 零缩放相机不产生脏区（防御退化输入）。
#[test]
fn degenerate_camera_yields_no_rect() {
    let muted = [false, false, true];
    let changed = tracks(&[1]);
    let mut degenerate = camera();
    degenerate.zoom_x = 0.0;
    let dirty = dirty_with(vec![change(
        1,
        None,
        Some(NoteEvent::new(10, 110, 121, 100, 0)),
    )]);
    let plan = plan_update(base(&degenerate, &muted, Some(&dirty), Some(&changed)));
    assert_eq!(plan.redraw, None);
}

/// 脏区面积超过整帧 1/4 → 整帧重绘（区域重绘已无收益）。
#[test]
fn large_dirty_rect_escalates_to_full() {
    let camera = camera();
    let muted = [false, false, true];
    let changed = tracks(&[1]);
    // 两条音符分别贴住画布上/下边界 → union 覆盖整列 240px，面积远超 1/4 帧
    let dirty = dirty_with(vec![
        change(1, None, Some(NoteEvent::new(0, 300, 127, 100, 0))),
        change(1, None, Some(NoteEvent::new(0, 300, 116, 100, 0))),
    ]);
    let plan = plan_update(base(&camera, &muted, Some(&dirty), Some(&changed)));
    assert_eq!(plan.redraw, Some(CpuOnionRedraw::Full));
}
