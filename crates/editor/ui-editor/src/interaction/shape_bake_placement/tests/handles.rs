//! DRAW-049 阶段 2：9 锚点（4 角 + 4 边 + 中心）拖动。
//!
//! 直接走 `begin_shape_edit` / `update_shape_edit` / `finish_shape_edit` 的
//! 逻辑坐标链路，避免屏幕换算误差。9 个锚点 = 4 角（双轴）、4 边（单轴）、
//! 中心（平移）。每个用例都要求：几何方向正确、预览音符随几何重生成、
//! 松手后恰好入一条 pending 编辑 undo。

use super::{draw_rect, pending_bounds, preview_note_snapshot, test_editor};
use crate::Editor;

/// 拖一个锚点：命中 → 拖动 → 松手，返回拖动后的 bounds。
fn drag_handle(editor: &mut Editor, from: (f32, f32), to: (f32, f32)) -> (f32, f32, f32, f32) {
    assert!(
        editor.begin_shape_edit(from.0, from.1),
        "应命中手柄 {from:?}"
    );
    assert!(editor.update_shape_edit(to.0, to.1), "拖动应更新几何");
    assert!(editor.finish_shape_edit(), "拖动应有实际变化");
    pending_bounds(editor)
}

/// 准备好一个处于放置态、已布防 8+1 锚点的编辑器。
fn placement_editor() -> (Editor, (f32, f32, f32, f32)) {
    let mut editor = test_editor();
    draw_rect(&mut editor);
    assert!(editor.begin_shape_bake_placement());
    assert!(editor.is_shape_edit_armed());
    let bounds = pending_bounds(&editor);
    (editor, bounds)
}

#[test]
fn corner_handles_resize_both_axes() {
    for (label, corner) in [("nw", 0u8), ("ne", 1), ("sw", 2), ("se", 3)] {
        let (mut editor, (x0, y0, x1, y1)) = placement_editor();
        let notes_before = preview_note_snapshot(&editor);
        let from = match corner {
            0 => (x0, y1),
            1 => (x1, y1),
            2 => (x0, y0),
            _ => (x1, y0),
        };
        let dx = if from.0 == x0 { -3.0 } else { 3.0 };
        let dy = if from.1 == y0 { -3.0 } else { 3.0 };

        let after = drag_handle(&mut editor, from, (from.0 + dx, from.1 + dy));

        assert!(
            after.2 - after.0 > (x1 - x0) + 1.0,
            "{label} 角应双轴加宽：{from:?} -> {after:?}"
        );
        assert!(
            after.3 - after.1 > (y1 - y0) + 1.0,
            "{label} 角应双轴加高：{from:?} -> {after:?}"
        );
        assert_ne!(
            preview_note_snapshot(&editor),
            notes_before,
            "{label} 角拖动后预览音符应重生成"
        );
        assert_eq!(
            editor.shape_tools.pending_edit_undo.len(),
            1,
            "{label} 角一次拖动应入一条 pending undo"
        );
    }
}

#[test]
fn side_handles_resize_single_axis() {
    for (label, side) in [("left", 0u8), ("right", 1), ("bottom", 2), ("top", 3)] {
        let (mut editor, (x0, y0, x1, y1)) = placement_editor();
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let width = x1 - x0;
        let height = y1 - y0;
        let from = match side {
            0 => (x0, cy),
            1 => (x1, cy),
            2 => (cx, y0),
            _ => (cx, y1),
        };
        let to = match side {
            0 => (from.0 - 3.0, from.1),
            1 => (from.0 + 3.0, from.1),
            2 => (from.0, from.1 - 3.0),
            _ => (from.0, from.1 + 3.0),
        };

        let after = drag_handle(&mut editor, from, to);

        if side < 2 {
            assert!(
                after.2 - after.0 > width + 1.0,
                "{label} 边应加宽：{from:?} -> {after:?}"
            );
            assert!(
                (after.3 - after.1 - height).abs() < 1e-3,
                "{label} 单边拉伸不得改高：{after:?}"
            );
        } else {
            assert!(
                after.3 - after.1 > height + 1.0,
                "{label} 边应加高：{from:?} -> {after:?}"
            );
            assert!(
                (after.2 - after.0 - width).abs() < 1e-3,
                "{label} 单边拉伸不得改宽：{after:?}"
            );
        }
        assert_eq!(
            editor.shape_tools.pending_edit_undo.len(),
            1,
            "{label} 边一次拖动应入一条 pending undo"
        );
    }
}

#[test]
fn center_handle_translates_without_resize() {
    let (mut editor, (x0, y0, x1, y1)) = placement_editor();
    let notes_before = preview_note_snapshot(&editor);
    let center = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);

    let after = drag_handle(&mut editor, center, (center.0 + 3.0, center.1 + 2.0));

    assert!(
        (after.0 - (x0 + 3.0)).abs() < 1e-3 && (after.1 - (y0 + 2.0)).abs() < 1e-3,
        "中心拖动应整体平移：{after:?}"
    );
    assert!(
        (after.2 - after.0 - (x1 - x0)).abs() < 1e-3,
        "中心拖动不得改宽：{after:?}"
    );
    assert!(
        (after.3 - after.1 - (y1 - y0)).abs() < 1e-3,
        "中心拖动不得改高：{after:?}"
    );
    assert_ne!(
        preview_note_snapshot(&editor),
        notes_before,
        "平移后预览音符应重生成"
    );
    assert_eq!(
        editor.shape_tools.pending_edit_undo.len(),
        1,
        "中心一次拖动应入一条 pending undo"
    );
}
