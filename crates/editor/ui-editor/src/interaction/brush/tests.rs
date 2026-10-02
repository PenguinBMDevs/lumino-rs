//! 画刷矢量笔画交互与提交测试
//!
//! 覆盖卡面验收要点：断墨（快速拖动连续）、拖动期间不写 document、
//! √ 一次历史记录、多笔画共存、层音轨分配（含基准轨语义）、Conductor 拦截、
//! 命名/颜色规则、批量归并路径。

use super::*;
use crate::message::{EditorAction, Point2};
use crate::tests::test_helpers::seed_notes;
use lumino_core::Tool;
use lumino_editor_state::brush_tool::cov;

/// 构造画刷编辑器：2 轨（Conductor + 1 普通轨），当前轨 = 1，精度 240
fn brush_editor() -> Editor {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Brush;
    seed_notes(&mut editor, 2, 1, &[]);
    editor.editor_state.view.snap_precision = 240.0;
    editor.editor_state.canvas.size_x = 800.0;
    editor.editor_state.canvas.size_y = 600.0;
    editor
}

/// 直接种一笔待确认笔画（等价于「按下 → 拖动 → 松手」的落点结果）
fn seed_stroke(editor: &mut Editor, points: &[(f32, f32)]) {
    let base = editor.editor_state.data.current_track;
    let Some((first, rest)) = points.split_first() else {
        return;
    };
    editor.editor_state.brush_tool.begin_stroke(*first, base);
    // 与交互路径一致：新笔画占一步笔画历史
    editor.editor_state.brush_tool.push_path_history();
    for point in rest {
        editor.editor_state.brush_tool.push_point(*point);
    }
    editor.editor_state.brush_tool.finish_stroke();
    editor.editor_state.brush_tool.update_top_path_history();
}

/// 某轨全部音符 `(tick, key)`（按文档顺序）
fn notes_of(editor: &Editor, track: usize) -> Vec<(f32, u8)> {
    editor
        .editor_state
        .data
        .track_notes(track)
        .iter()
        .map(|n| (n.start_tick as f32, n.key))
        .collect()
}

// ── 断墨（快速拖动） ─────────────────────────

#[test]
fn test_fast_drag_one_frame_covers_every_cell_on_confirm() {
    let mut editor = brush_editor();
    // 一帧从第 0 格跨到第 10 格（旧实现只盖首尾两格 → 中间 9 格空洞）
    seed_stroke(&mut editor, &[(0.0, 60.0), (2400.0, 60.0)]);
    assert!(editor.confirm_brush());

    let notes = notes_of(&editor, 1);
    let ticks: Vec<f32> = notes.iter().map(|n| n.0).collect();
    assert_eq!(
        ticks,
        (0..=10).map(|i| i as f32 * 240.0).collect::<Vec<f32>>(),
        "快速拖动必须逐格生成，无断墨空洞"
    );
    assert!(notes.iter().all(|n| n.1 == 60), "同 key 水平笔画");
}

#[test]
fn test_steep_drag_is_connected_on_confirm() {
    let mut editor = brush_editor();
    seed_stroke(&mut editor, &[(0.0, 60.0), (960.0, 66.0)]);
    assert!(editor.confirm_brush());

    let mut cells: Vec<cov::CoveredCell> = notes_of(&editor, 1)
        .iter()
        .map(|&(tick, key)| ((tick / 240.0) as i64, key as u16))
        .collect();
    // 覆盖集（单层）应当与覆盖函数一致：即"生成 = 预览所用覆盖集"
    let mut expected = cov::cover_cells(&[(0.0, 60.0), (960.0, 66.0)], 240.0);
    cells.sort_unstable();
    expected.sort_unstable();
    assert_eq!(cells, expected, "生成音符格集合 == 覆盖函数输出");
    assert!(cells.contains(&(0, 60)) && cells.contains(&(4, 66)));
}

// ── 拖动期间不写 document ─────────────────────────

#[test]
fn test_drawing_writes_nothing_to_document() {
    let mut editor = brush_editor();
    let start = editor.line_pos_screen_pos((0.0, 60.0));
    editor.handle_brush_pressed(start, None, 60);
    for step in 1..=8 {
        let pos = editor.line_pos_screen_pos((step as f32 * 480.0, 60.0));
        editor.handle_brush_moved(pos);
    }
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        0,
        "落笔拖动全程不得写入 document"
    );
    assert!(
        !editor.editor_state.data.history.can_undo(),
        "未确认前不得产生 document 历史"
    );
    editor.handle_released();
    assert!(
        editor.editor_state.brush_tool.has_pending(),
        "松手进入待确认"
    );
    assert!(editor.can_undo(), "笔画编辑历史独立可用");
}

// ── √ 一次历史记录 ─────────────────────────

#[test]
fn test_confirm_creates_one_history_entry_undone_at_once() {
    let mut editor = brush_editor();
    editor.brush.set_thickness(3);
    seed_stroke(&mut editor, &[(0.0, 60.0), (960.0, 60.0)]);
    seed_stroke(&mut editor, &[(0.0, 70.0), (960.0, 70.0)]);
    assert!(editor.confirm_brush());

    let before = notes_of(&editor, 1).len();
    assert!(before > 0, "√ 应生成音符");
    assert!(
        !editor.editor_state.brush_tool.has_pending(),
        "确认后清空笔画"
    );

    assert!(editor.undo(), "Ctrl+Z 一次回退全部生成音符");
    assert!(
        notes_of(&editor, 1).is_empty(),
        "一次撤销必须清掉全部笔画生成的全部音符（{before} 个）"
    );
    assert!(!editor.editor_state.data.history.can_undo(), "恰好一条记录");
}

#[test]
fn test_cancel_discards_all_strokes_without_history() {
    let mut editor = brush_editor();
    seed_stroke(&mut editor, &[(0.0, 60.0), (960.0, 60.0)]);
    seed_stroke(&mut editor, &[(0.0, 70.0), (960.0, 70.0)]);
    editor.cancel_brush();

    assert!(!editor.editor_state.brush_tool.has_pending());
    assert!(notes_of(&editor, 1).is_empty(), "× 不得生成音符");
    assert!(
        !editor.editor_state.data.history.can_undo(),
        "× 不得产生历史"
    );
}

// ── 层音轨分配 ─────────────────────────

#[test]
fn test_layer_tracks_follow_explicit_config() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Brush;
    seed_notes(&mut editor, 4, 1, &[]);
    editor.editor_state.view.snap_precision = 240.0;
    editor.brush.set_thickness(3);
    editor.brush.set_track(0, Some(1));
    editor.brush.set_track(1, Some(2));
    editor.brush.set_track(2, Some(3));

    seed_stroke(&mut editor, &[(0.0, 60.0)]);
    assert!(editor.confirm_brush());

    assert_eq!(notes_of(&editor, 1), vec![(0.0, 60u8)], "层 0 → 轨 1");
    assert_eq!(notes_of(&editor, 2), vec![(0.0, 61u8)], "层 1 → 轨 2");
    assert_eq!(notes_of(&editor, 3), vec![(0.0, 62u8)], "层 2 → 轨 3");
}

#[test]
fn test_default_layer_assignment_uses_stroke_base_track() {
    // 4 轨（3 条普通轨）：落笔在轨 1，随后切到轨 2 —— 目标轨必须仍按落笔基准轨解析
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Brush;
    seed_notes(&mut editor, 4, 1, &[]);
    editor.editor_state.view.snap_precision = 240.0;
    editor.brush.set_thickness(2);

    seed_stroke(&mut editor, &[(0.0, 60.0)]);
    editor.editor_state.data.current_track = 2; // 画完切轨（预览/生成口径一致）
    assert!(editor.confirm_brush());

    assert_eq!(notes_of(&editor, 1), vec![(0.0, 60u8)], "层 0 回到基准轨 1");
    assert_eq!(notes_of(&editor, 2), vec![(0.0, 61u8)], "层 1 = 基准轨 +1");
    assert!(notes_of(&editor, 3).is_empty(), "不得按新当前轨分配");
}

#[test]
fn test_thickness_expands_upward_and_clamps_at_255() {
    let mut editor = brush_editor();
    editor.brush.set_thickness(4);
    seed_stroke(&mut editor, &[(0.0, 253.0)]);
    assert!(editor.confirm_brush());
    let keys: Vec<u8> = notes_of(&editor, 1).iter().map(|n| n.1).collect();
    assert_eq!(keys, vec![253, 254, 255], "向上展开并截断到 255");
}

// ── 多笔画 / Conductor / 切工具 ─────────────────────────

#[test]
fn test_multiple_strokes_share_one_button_pair_and_apply_together() {
    let mut editor = brush_editor();
    seed_stroke(&mut editor, &[(0.0, 60.0)]);
    seed_stroke(&mut editor, &[(480.0, 64.0)]);
    seed_stroke(&mut editor, &[(960.0, 68.0)]);
    assert_eq!(editor.editor_state.brush_tool.strokes.len(), 3);

    let btns = crate::grid::brush_tool_box::brush_button_rects(&editor).expect("应有一对按钮");
    assert!(btns.confirm.width > 0.0 && btns.cancel.width > 0.0);

    assert!(editor.confirm_brush());
    assert_eq!(notes_of(&editor, 1).len(), 3, "√ 一次性应用全部笔画");
    assert!(editor.editor_state.brush_tool.strokes.is_empty());
}

#[test]
fn test_conductor_track_blocks_new_stroke() {
    let mut editor = brush_editor();
    editor.editor_state.data.current_track = 0;
    let pos = editor.line_pos_screen_pos((0.0, 60.0));
    editor.handle_brush_pressed(pos, None, 60);
    assert!(
        !editor.editor_state.brush_tool.has_pending(),
        "Conductor 轨禁止落笔（与曲线工具对齐）"
    );
}

#[test]
fn test_pending_strokes_survive_track_switch_to_conductor() {
    // 待确认期间切到 Conductor：按钮与 √/× 仍可用（否则笔画被卡死）
    let mut editor = brush_editor();
    seed_stroke(&mut editor, &[(0.0, 60.0)]);
    editor.editor_state.data.current_track = 0;
    assert!(crate::grid::brush_tool_box::brush_button_rects(&editor).is_some());
    assert!(editor.confirm_brush(), "待确认笔画应能正常确认");
    assert_eq!(notes_of(&editor, 1).len(), 1);
}

#[test]
fn test_switch_tool_discards_pending_strokes() {
    let mut editor = brush_editor();
    seed_stroke(&mut editor, &[(0.0, 60.0)]);
    editor.set_tool(Tool::Pencil);
    assert!(
        !editor.editor_state.brush_tool.has_pending(),
        "切工具视为 ×"
    );
}

// ── 拖动笔画 ─────────────────────────

#[test]
fn test_press_on_stroke_body_starts_drag_not_new_stroke() {
    let mut editor = brush_editor();
    seed_stroke(&mut editor, &[(0.0, 60.0), (2400.0, 60.0)]);

    let body = editor.line_pos_screen_pos((1200.0, 60.0));
    editor.handle_brush_pressed(body, None, 60);
    assert!(
        editor.editor_state.brush_tool.is_dragging(),
        "命中实心区应进入拖动"
    );
    assert_eq!(
        editor.editor_state.brush_tool.strokes.len(),
        1,
        "不得新建笔画"
    );

    let target = editor.line_pos_screen_pos((1337.0, 62.4));
    editor.handle_brush_moved(target);
    editor.handle_released();

    let points = editor.editor_state.brush_tool.strokes[0].points.clone();
    assert_eq!(points[0].1, 62.0, "Y 向按单个 key 吸附");
    assert!(
        (points[0].0 - 137.0).abs() < 1.0,
        "X 向自由：{:?}",
        points[0]
    );
    assert!(!editor.editor_state.brush_tool.is_active());
    assert!(
        editor.editor_state.brush_tool.has_pending(),
        "拖动后仍待确认"
    );
}

#[test]
fn test_dragged_position_is_what_confirm_generates() {
    let mut editor = brush_editor();
    seed_stroke(&mut editor, &[(0.0, 60.0), (2400.0, 60.0)]);
    let body = editor.line_pos_screen_pos((1200.0, 60.0));
    editor.handle_brush_pressed(body, None, 60);
    let target = editor.line_pos_screen_pos((1337.0, 62.4));
    editor.handle_brush_moved(target);
    editor.handle_released();

    let points = editor.editor_state.brush_tool.strokes[0].points.clone();
    assert!(editor.confirm_brush());
    let mut cells: Vec<cov::CoveredCell> = notes_of(&editor, 1)
        .iter()
        .map(|&(tick, key)| ((tick / 240.0) as i64, key as u16))
        .collect();
    // 生成结果 = 拖动后点列的覆盖集（预览与生成同口径）
    let mut expected = cov::cover_cells(&points, 240.0);
    cells.sort_unstable();
    expected.sort_unstable();
    assert_eq!(cells, expected, "√ 生成位置与预览一致");
}

// ── 笔画历史（独立于 document） ─────────────────────────

#[test]
fn test_stroke_history_undo_redo_before_confirm() {
    let mut editor = brush_editor();
    seed_stroke(&mut editor, &[(0.0, 60.0), (240.0, 60.0)]);
    assert_eq!(editor.editor_state.brush_tool.strokes.len(), 1);

    assert!(editor.undo(), "撤销笔画编辑");
    assert!(editor.editor_state.brush_tool.strokes.is_empty());
    assert!(!editor.can_undo(), "已无更多历史（document 亦无）");

    assert!(editor.redo(), "重做笔画编辑");
    assert_eq!(editor.editor_state.brush_tool.strokes.len(), 1);
    assert!(!editor.can_redo());
}

// ── 颜色与宽度 ─────────────────────────

#[test]
fn test_darken_is_40_percent() {
    let color = darken(
        iced_core::Color::from_rgba(1.0, 0.5, 0.25, 1.0),
        LAYER_DARKEN,
    );
    assert!((color.r - 0.6).abs() < 1e-6, "1.0 × 0.6");
    assert!((color.g - 0.3).abs() < 1e-6, "0.5 × 0.6");
    assert!((color.b - 0.15).abs() < 1e-6, "0.25 × 0.6");
    assert_eq!(color.a, 1.0, "alpha 不变");
}

#[test]
fn test_layer_color_comes_from_assigned_track_display_color() {
    let mut editor = Editor::new();
    seed_notes(&mut editor, 4, 1, &[]);
    editor.brush.set_thickness(2);
    editor.brush.set_track(0, Some(1));
    editor.brush.set_track(1, Some(3));

    let expected0 = {
        let c = lumino_extras::palette::current_track_color_f32(1);
        darken(
            iced_core::Color::from_rgba(c[0], c[1], c[2], c[3]),
            LAYER_DARKEN,
        )
    };
    let expected1 = {
        let c = lumino_extras::palette::current_track_color_f32(3);
        darken(
            iced_core::Color::from_rgba(c[0], c[1], c[2], c[3]),
            LAYER_DARKEN,
        )
    };
    let color0 = editor.brush_track_color(editor.brush_track_for_level(0, 1));
    let color1 = editor.brush_track_color(editor.brush_track_for_level(1, 1));
    assert!((color0.r - expected0.r).abs() < 1e-6 && (color0.g - expected0.g).abs() < 1e-6);
    assert!((color1.r - expected1.r).abs() < 1e-6 && (color1.g - expected1.g).abs() < 1e-6);
    assert!(
        color0.r != color1.r || color0.g != color1.g || color0.b != color1.b,
        "不同轨层色不同（硬切换，非渐变）"
    );
}

#[test]
fn test_total_width_follows_zoom_and_thickness() {
    let mut editor = brush_editor();
    editor.brush.set_thickness(3);
    editor.editor_state.view.zoom_y = 20.0;
    assert!(
        (editor.brush_total_width_px() - 60.0).abs() < 1e-6,
        "3 × 20px"
    );

    editor.editor_state.view.zoom_y = 8.0;
    assert!(
        (editor.brush_total_width_px() - 24.0).abs() < 1e-6,
        "缩放后实时重算"
    );

    editor.brush.set_thickness(1);
    assert!(
        (editor.brush_total_width_px() - 8.0).abs() < 1e-6,
        "粗细度 1 = 单 key 高"
    );
}

// ── 批量归并路径（大规模 √） ─────────────────────────

#[test]
fn test_large_confirm_uses_batch_path_and_is_complete() {
    let mut editor = brush_editor();
    editor.brush.set_thickness(20);
    // 151 格 × 20 层 = 3020 音符 > BATCH_INSERT_THRESHOLD → 走批量归并
    seed_stroke(&mut editor, &[(0.0, 60.0), (150.0 * 240.0, 60.0)]);
    assert!(editor.confirm_brush());

    let notes = notes_of(&editor, 1);
    assert_eq!(notes.len(), 151 * 20, "批量路径不得丢音符");
    assert!(notes.contains(&(0.0, 60u8)) && notes.contains(&(36000.0, 79u8)));

    assert!(editor.undo(), "批量路径同样只占一条历史记录");
    assert!(notes_of(&editor, 1).is_empty());
}

// ── √/× 按钮定位 ─────────────────────────

#[test]
fn test_button_rects_only_for_brush_with_pending_strokes() {
    let mut editor = brush_editor();
    assert!(
        crate::grid::brush_tool_box::brush_button_rects(&editor).is_none(),
        "无笔画不显示按钮"
    );
    seed_stroke(&mut editor, &[(0.0, 60.0)]);
    assert!(crate::grid::brush_tool_box::brush_button_rects(&editor).is_some());

    editor.set_tool(Tool::Pencil);
    assert!(
        crate::grid::brush_tool_box::brush_button_rects(&editor).is_none(),
        "非画刷工具不显示按钮"
    );
}

#[test]
fn test_hit_band_aligns_with_visual_blocks() {
    // 粗细度 5：方块占据 [落笔 key, 落笔 key+5)（向上铺）
    let mut editor = brush_editor();
    editor.brush.set_thickness(5);
    seed_stroke(&mut editor, &[(0.0, VISIBLE_KEY), (2400.0, VISIBLE_KEY)]);

    // 方块带中心（key + 2）附近：必须命中
    let band_center = editor.line_pos_screen_pos((1200.0, VISIBLE_KEY + 2.0));
    assert!(
        editor.brush_stroke_hit_test(band_center).is_some(),
        "方块带中心应命中"
    );
    // 方块带下方（落笔 key - 3，远离方块）不得命中
    let below = editor.line_pos_screen_pos((1200.0, VISIBLE_KEY - 3.0));
    assert!(
        editor.brush_stroke_hit_test(below).is_none(),
        "方块带之外不得命中（所见即所按）"
    );
}

// ── 待确认笔画的撤销/重做（走真实 EditorAction 链路） ─────────────────────────

/// 画一笔（真实链路 Pressed → Moved… → Released）
///
/// key 必须落在画布内（`handle_action` 会经 `is_inside_canvas` 守卫），
/// 故调用方传 `VISIBLE_KEY` 附近的 key。
fn stroke_via_actions(editor: &mut Editor, points: &[(f32, f32)]) {
    let start = editor.line_pos_screen_pos(points[0]);
    editor.handle_action(EditorAction::Pressed {
        pos: Point2::new(start.x, start.y),
        shift: false,
        ctrl: false,
    });
    for point in points.iter().skip(1) {
        let screen = editor.line_pos_screen_pos(*point);
        editor.handle_action(EditorAction::Moved(Point2::new(screen.x, screen.y)));
    }
    editor.handle_action(EditorAction::Released);
}

/// 画布内可见的 key（画布 600 高 / zoom_y 20：key 100 位于 y=564）
const VISIBLE_KEY: f32 = 100.0;

#[test]
fn test_pending_strokes_undo_redo_one_by_one() {
    let mut editor = brush_editor();
    for key in [VISIBLE_KEY, VISIBLE_KEY + 4.0, VISIBLE_KEY + 8.0] {
        stroke_via_actions(&mut editor, &[(0.0, key), (2400.0, key)]);
    }
    assert_eq!(editor.editor_state.brush_tool.strokes.len(), 3);
    assert!(
        crate::grid::brush_tool_box::brush_button_rects(&editor).is_some(),
        "待确认笔画应显示 √× 按钮"
    );

    // 逐笔撤销：每次只掉一笔
    for expected in [2usize, 1, 0] {
        editor.handle_action(EditorAction::Undo);
        assert_eq!(
            editor.editor_state.brush_tool.strokes.len(),
            expected,
            "Ctrl+Z 必须逐笔撤销"
        );
    }
    assert!(
        !editor.can_undo(),
        "笔画撤空后无更多可撤销（document 历史亦为空）"
    );
    assert!(
        crate::grid::brush_tool_box::brush_button_rects(&editor).is_none(),
        "笔画撤空后按钮消失"
    );

    // 逐笔重做：每次只回一笔
    for expected in [1usize, 2, 3] {
        editor.handle_action(EditorAction::Redo);
        assert_eq!(
            editor.editor_state.brush_tool.strokes.len(),
            expected,
            "Ctrl+Y 必须逐笔重做"
        );
    }
    assert!(!editor.can_redo());
    assert!(
        !editor.editor_state.data.history.can_undo(),
        "笔画期操作不得写入 document 历史"
    );
}

#[test]
fn test_drag_stroke_is_one_undo_step() {
    let mut editor = brush_editor();
    stroke_via_actions(&mut editor, &[(0.0, VISIBLE_KEY), (2400.0, VISIBLE_KEY)]);
    let before = editor.editor_state.brush_tool.strokes[0].points.clone();

    // 按住笔画实心区拖动（X 自由、Y 单 key 吸附）
    let body = editor.line_pos_screen_pos((1200.0, VISIBLE_KEY));
    editor.handle_action(EditorAction::Pressed {
        pos: Point2::new(body.x, body.y),
        shift: false,
        ctrl: false,
    });
    let target = editor.line_pos_screen_pos((1337.0, VISIBLE_KEY + 2.4));
    editor.handle_action(EditorAction::Moved(Point2::new(target.x, target.y)));
    editor.handle_action(EditorAction::Released);

    let moved = editor.editor_state.brush_tool.strokes[0].points.clone();
    assert_ne!(moved, before, "拖动应改变笔画位置");
    assert_eq!(moved[0].1, VISIBLE_KEY + 2.0, "Y 向按单个 key 吸附");

    // 一次 Ctrl+Z 回到拖动前；一次 Ctrl+Y 回到拖动后
    editor.handle_action(EditorAction::Undo);
    assert_eq!(
        editor.editor_state.brush_tool.strokes[0].points, before,
        "整段拖动 = 一步撤销"
    );
    editor.handle_action(EditorAction::Redo);
    assert_eq!(editor.editor_state.brush_tool.strokes[0].points, moved);
}

#[test]
fn test_noop_drag_adds_no_dead_undo_step() {
    let mut editor = brush_editor();
    stroke_via_actions(&mut editor, &[(0.0, VISIBLE_KEY), (2400.0, VISIBLE_KEY)]);

    // 按住笔画实心区但不移动 → 不应产生"按了没反应"的空撤销步
    let body = editor.line_pos_screen_pos((1200.0, VISIBLE_KEY));
    editor.handle_action(EditorAction::Pressed {
        pos: Point2::new(body.x, body.y),
        shift: false,
        ctrl: false,
    });
    editor.handle_action(EditorAction::Released);

    editor.handle_action(EditorAction::Undo);
    assert!(
        editor.editor_state.brush_tool.strokes.is_empty(),
        "空拖动不得占用撤销步（一次 Ctrl+Z 应直接撤销该笔）"
    );
}

#[test]
fn test_cancel_discards_stroke_history_too() {
    let mut editor = brush_editor();
    stroke_via_actions(&mut editor, &[(0.0, VISIBLE_KEY)]);
    stroke_via_actions(&mut editor, &[(480.0, VISIBLE_KEY + 2.0)]);
    assert!(editor.can_undo(), "有待确认笔画即有笔画历史");

    editor.handle_action(EditorAction::BrushCancel);
    assert!(!editor.editor_state.brush_tool.has_pending());
    assert!(
        !editor.can_undo() && !editor.can_redo(),
        "× 取消后笔画历史栈一并清空"
    );
    assert!(
        !editor.editor_state.data.history.can_undo(),
        "× 不产生 document 历史"
    );
}

#[test]
fn test_confirm_then_undo_removes_all_notes_and_strokes_stay_gone() {
    let mut editor = brush_editor();
    editor.brush.set_thickness(2);
    stroke_via_actions(&mut editor, &[(0.0, VISIBLE_KEY), (2400.0, VISIBLE_KEY)]);
    stroke_via_actions(
        &mut editor,
        &[(0.0, VISIBLE_KEY + 2.0), (2400.0, VISIBLE_KEY + 2.0)],
    );

    editor.handle_action(EditorAction::BrushConfirm);
    let generated = notes_of(&editor, 1).len();
    assert!(generated > 0, "√ 应生成音符");
    assert!(
        !editor.editor_state.brush_tool.has_pending(),
        "√ 后清空笔画"
    );
    assert!(
        !editor.editor_state.brush_tool.can_undo_path(),
        "√ 后笔画历史栈清空（document 历史单独存在）"
    );
    assert!(
        editor.editor_state.data.history.can_undo(),
        "√ 写入 document 产生一条记录"
    );

    // Ctrl+Z：一次回退全部生成音符（卡片要求），且笔画**不会**被还原
    editor.handle_action(EditorAction::Undo);
    assert!(notes_of(&editor, 1).is_empty(), "一次撤销回退全部生成音符");
    assert!(
        !editor.editor_state.brush_tool.has_pending(),
        "√ 后撤销不得把笔画还原成待确认状态"
    );

    // Ctrl+Y：音符回来
    editor.handle_action(EditorAction::Redo);
    assert_eq!(notes_of(&editor, 1).len(), generated, "Ctrl+Y 恢复生成音符");
}
