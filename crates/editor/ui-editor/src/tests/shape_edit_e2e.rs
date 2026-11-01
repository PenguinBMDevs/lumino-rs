//! 020「编辑图形」端到端：走真实输入链（`handle_action`）+ 屏幕坐标。
//!
//! 目的：锁住「即拖即走」——按下锚点后**第一次** Moved 就必须改几何，
//! 而不是必须再点一次才生效（`moved`/`released` 漏判 `is_shape_editing`）。

use crate::Editor;
use crate::Note;
use crate::tests::test_helpers::seed_notes;
use iced_core::Point;
use lumino_editor_state::{DrawnShapeSource, ShapeKind, ShapeNote};
use lumino_message::Tool;
use lumino_ui_core::message::{EditorAction, Point2};

/// 已 Bake 矩形（200..1200 tick × key 60..64）+ 一条对应文档音符
fn editor_with_shape() -> Editor {
    let mut editor = Editor::new();
    seed_notes(&mut editor, 2, 1, &[Note::new(200.0, 60, 1.0)]);
    editor.editor_state.view.snap_precision = 1.0;
    editor.set_canvas_size(Point::new(4000.0, 4000.0));
    editor.set_canvas_offset(Point::new(0.0, 0.0));
    let track = editor.editor_state.data.current_track;
    let id = editor.editor_state.shape_select.add(
        track,
        None,
        DrawnShapeSource::Shape {
            kind: ShapeKind::Rectangle,
            rect: (200.0, 60.0, 1200.0, 64.0),
            shift_constrained: false,
            filled: false,
            apex_high: true,
        },
        vec![ShapeNote {
            track: 1,
            tick: 200.0,
            key: 60,
            length: 1.0,
            velocity: 100,
            channel: 0,
        }],
    );
    editor.set_tool(Tool::ShapeSelect);
    editor.editor_state.shape_select.select_only(id);
    assert!(editor.ensure_model_for_drawn(id).is_some());
    assert!(editor.enable_shape_edit(), "应进入编辑图形");
    editor
}

fn press_at(editor: &mut Editor, screen: Point) {
    editor.handle_action(EditorAction::Pressed {
        pos: Point2::new(screen.x, screen.y),
        shift: false,
        ctrl: false,
    });
}

fn move_to(editor: &mut Editor, screen: Point) {
    editor.handle_action(EditorAction::Moved(Point2::new(screen.x, screen.y)));
}

fn release(editor: &mut Editor) {
    editor.handle_action(EditorAction::Released);
}

/// 当前选中图形的逻辑外接框（拖拽中读 source，提交后同样读 source）
fn bounds_of(editor: &Editor) -> (f32, f32, f32, f32) {
    match &editor.editor_state.shape_select.shapes()[0].source {
        DrawnShapeSource::Shape { rect, .. } => *rect,
        DrawnShapeSource::Polyline { points } => {
            let mut it = points.iter();
            let first = *it.next().expect("折线至少一个点");
            let (mut x0, mut y0, mut x1, mut y1) = (first.0, first.1, first.0, first.1);
            for &(x, y) in it {
                x0 = x0.min(x);
                x1 = x1.max(x);
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
            (x0, y0, x1, y1)
        }
        other => panic!("期望矩形/折线，实际 {other:?}"),
    }
}

/// 逻辑外接框 → 屏幕上的像素宽高（横/纵卷帘都由 `line_pos_screen_pos` 兜底）
fn screen_size(editor: &Editor, bounds: (f32, f32, f32, f32)) -> (f32, f32) {
    let a = editor.line_pos_screen_pos((bounds.0, bounds.1));
    let b = editor.line_pos_screen_pos((bounds.2, bounds.3));
    ((b.x - a.x).abs(), (b.y - a.y).abs())
}

#[test]
fn test_anchor_drag_moves_on_first_move() {
    let mut editor = editor_with_shape();
    let before = bounds_of(&editor);
    let nw = editor.line_pos_screen_pos((before.0, before.3));
    press_at(&mut editor, nw);
    assert!(editor.is_shape_editing(), "按下角锚点应立即进入拖拽");

    // 关键：第一次 Moved 就要生效（不能要求先点一下）
    let target = editor.line_pos_screen_pos((before.0, before.3 + 2.0));
    move_to(&mut editor, target);
    assert!(editor.is_shape_editing(), "拖动过程中应保持编辑拖拽状态");
    let during = bounds_of(&editor);
    assert!(
        (during.3 - (before.3 + 2.0)).abs() < 1e-3,
        "第一次 Moved 后几何应已变化：{before:?} → {during:?}"
    );

    release(&mut editor);
    assert!(!editor.is_shape_editing(), "松手应结束拖拽");
    let after = bounds_of(&editor);
    assert!(
        (after.3 - (before.3 + 2.0)).abs() < 1e-3,
        "松手应提交：{after:?}"
    );
}

#[test]
fn test_click_outside_edit_box_keeps_edit_mode() {
    let mut editor = editor_with_shape();
    let rect = editor.shape_edit_box().expect("编辑框");
    let outside = Point::new(rect.x + rect.width + 120.0, rect.y + rect.height + 80.0);

    press_at(&mut editor, outside);
    release(&mut editor);

    assert!(
        editor.is_shape_edit_armed(),
        "单击编辑框外不得退出编辑图形（避免一次没抓准就丢状态）"
    );
    assert_eq!(
        editor.editor_state.shape_select.selection_len(),
        1,
        "选中集应保持不变"
    );
    assert!(
        !editor.editor_state.shape_select.is_marqueeing(),
        "编辑态不应开始框选"
    );
}

#[test]
fn test_double_click_outside_exits_edit_mode() {
    let mut editor = editor_with_shape();
    let rect = editor.shape_edit_box().expect("编辑框");
    let outside = Point::new(rect.x + rect.width + 120.0, rect.y + rect.height + 80.0);

    editor.handle_action(EditorAction::DoubleClicked(Point2::new(
        outside.x, outside.y,
    )));
    assert!(!editor.is_shape_edit_armed(), "双击应退出编辑图形");
    assert!(
        editor.selection_box().is_some(),
        "退出后 ShapeSelect 选框应回来"
    );
}

#[test]
fn test_corner_outside_ring_rotates_in_screen_space() {
    let mut editor = editor_with_shape();
    let before = bounds_of(&editor);
    let rect = editor.shape_edit_box().expect("编辑框");
    // 右上角外 20px：横纵都出框 → 旋转热区（不是缩放）
    let press = Point::new(rect.x + rect.width + 20.0, rect.y - 20.0);
    press_at(&mut editor, press);
    assert!(editor.is_shape_editing(), "角外热区应进入旋转");

    let center = Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    let vx = press.x - center.x;
    let vy = press.y - center.y;
    // 屏幕绕中心 +90°（逻辑上是逆时针 90°）
    let target = Point::new(center.x + vy, center.y - vx);
    move_to(&mut editor, target);
    release(&mut editor);

    let after = bounds_of(&editor);
    let (bw, bh) = screen_size(&editor, before);
    let (aw, ah) = screen_size(&editor, after);
    assert!(
        (aw - bh).abs() < 6.0 && (ah - bw).abs() < 6.0,
        "90° 旋转后屏幕宽高应互换：{bw}x{bh} → {aw}x{ah}（{before:?} → {after:?}）"
    );
}

#[test]
fn test_drag_inside_edit_box_still_moves_whole_shape() {
    let mut editor = editor_with_shape();
    let before = bounds_of(&editor);
    let cx = (before.0 + before.2) / 2.0;
    let cy = (before.1 + before.3) / 2.0;
    let start = editor.line_pos_screen_pos((cx, cy));
    press_at(&mut editor, start);
    assert!(
        editor.editor_state.shape_select.is_dragging(),
        "编辑框内按下应进入整组拖动（已 Bake 没有中心锚点）"
    );
    let target = editor.line_pos_screen_pos((cx + 10.0, cy));
    move_to(&mut editor, target);
    release(&mut editor);

    let after = bounds_of(&editor);
    assert!(
        (after.0 - (before.0 + 10.0)).abs() < 1e-3,
        "整组拖动应生效：{before:?} → {after:?}"
    );
}
