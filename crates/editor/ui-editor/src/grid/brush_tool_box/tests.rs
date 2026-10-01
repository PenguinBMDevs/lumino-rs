//! 画刷笔画渲染层测试：方块几何 / 行段合并 / 增量稳定（抖动回归）/ 按钮定位

use super::*;
use crate::tests::test_helpers::seed_notes;
use lumino_core::Tool;

/// 构造画刷编辑器（2 轨，当前轨 1，画布 800x600，精度 240）
fn brush_editor() -> Editor {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Brush;
    seed_notes(&mut editor, 2, 1, &[]);
    editor.editor_state.view.snap_precision = 240.0;
    editor.editor_state.canvas.size_x = 800.0;
    editor.editor_state.canvas.size_y = 600.0;
    editor
}

/// 种一笔待确认笔画（逻辑坐标折线）
fn seed_stroke(editor: &mut Editor, points: &[(f32, f32)]) {
    let Some((first, rest)) = points.split_first() else {
        return;
    };
    let base = editor.editor_state.data.current_track;
    editor.editor_state.brush_tool.begin_stroke(*first, base);
    editor.editor_state.brush_tool.push_path_history();
    for point in rest {
        editor.editor_state.brush_tool.push_point(*point);
    }
    editor.editor_state.brush_tool.finish_stroke();
    editor.editor_state.brush_tool.update_top_path_history();
}

/// 默认视图下位于画布内的 key（画布 600 高、zoom_y 20 → 顶部约 30 个 key 可见）
const VISIBLE_KEY: f32 = 100.0;

// ── 方块几何（方形、所见即生成） ─────────────────────────

#[test]
fn test_cell_rect_is_exactly_one_cell() {
    let mut editor = brush_editor();
    editor.editor_state.view.zoom_x = 0.1;
    editor.editor_state.view.zoom_y = 20.0;

    let rect = brush_cell_rect(&editor, 3, 4, 100);
    assert!(
        (rect.width - 24.0).abs() < 1e-3,
        "横向宽度 = 吸附精度 × zoom_x = 24px，实际 {}",
        rect.width
    );
    assert!(
        (rect.height - 20.0).abs() < 1e-3,
        "横向高度 = 单 key 高（无端帽、无圆角）"
    );

    // 纵向转置：宽度 = key 高，高度 = 格宽
    editor.editor_state.is_vertical_roll = true;
    let vrect = brush_cell_rect(&editor, 3, 4, 100);
    assert!((vrect.width - 20.0).abs() < 1e-3, "纵向宽度 = 单 key 高");
    assert!((vrect.height - 24.0).abs() < 1e-3, "纵向高度 = 格宽");
}

#[test]
fn test_preview_runs_merge_consecutive_cells_same_row() {
    let mut editor = brush_editor();
    // 一行连续 6 格（tick 格 0..=5）→ 合并为 1 个行段
    seed_stroke(&mut editor, &[(0.0, 100.0), (5.0 * 240.0, 100.0)]);
    let runs = editor.brush_preview_runs();
    assert_eq!(runs, vec![(1, 100, 0, 5)], "同轨同行连续格合并为一段");
}

#[test]
fn test_preview_runs_split_across_rows_and_tracks() {
    let mut editor = brush_editor();
    editor.brush.set_thickness(2);
    // 单点笔画：2 层 → 两个 key 行（同一音轨、不同 key → 不合并）
    seed_stroke(&mut editor, &[(0.0, 100.0)]);
    let runs = editor.brush_preview_runs();
    assert_eq!(
        runs,
        vec![(1, 100, 0, 0), (1, 101, 0, 0)],
        "不同 key 各自成段"
    );
}

#[test]
fn test_preview_runs_empty_without_pending_strokes() {
    let mut editor = brush_editor();
    assert!(editor.brush_preview_runs().is_empty());
    seed_stroke(&mut editor, &[(0.0, 100.0)]);
    assert_eq!(editor.brush_preview_runs().len(), 1);
    editor.cancel_brush();
    assert!(editor.brush_preview_runs().is_empty(), "× 后预览清空");
}

// ── ★ 抖动回归：笔画变长时已有方块必须原地不动 ─────────────────────────

#[test]
fn test_preview_runs_are_stable_while_stroke_grows() {
    // 旧折线渲染的根因：超过点数上限后等距重采样按"当前长度"重映射索引
    // → 追加一个点就让整条折线的顶点漂移（实测 n=1500→1501 时 512 个取样点里
    //   只有 256 个原地不动 = 整笔抖动）。方块渲染只增不改，必须逐段原地不动。
    let mut editor = brush_editor();
    let mut points: Vec<(f32, f32)> = (0..400)
        .map(|i| (i as f32 * 60.0, VISIBLE_KEY + (i % 7) as f32))
        .collect();
    seed_stroke(&mut editor, &points);
    let before = editor.brush_preview_runs();
    assert!(!before.is_empty());

    // 追加一个采样点（模拟继续拖动）
    points.push((401.0 * 60.0, VISIBLE_KEY + 3.0));
    editor
        .editor_state
        .brush_tool
        .push_point(*points.last().expect("刚 push"));

    let after = editor.brush_preview_runs();
    for run in &before {
        assert!(
            after.contains(run),
            "追加采样点不得移动/删除已有方块：{run:?} 在 {after:?} 中消失"
        );
    }
    assert!(after.len() >= before.len(), "只允许新增方块");
}

#[test]
fn test_preview_runs_grow_by_addition_when_dragging_fast() {
    let mut editor = brush_editor();
    // 一帧跨 10 格（断墨场景）：行段应覆盖 0..=10 格
    seed_stroke(&mut editor, &[(0.0, 100.0), (10.0 * 240.0, 100.0)]);
    let runs = editor.brush_preview_runs();
    assert_eq!(runs, vec![(1, 100, 0, 10)]);
}

// ── 所见即生成（预览 == √ 生成的音符） ─────────────────────────

#[test]
fn test_preview_runs_match_confirmed_notes_exactly() {
    let mut editor = brush_editor();
    editor.brush.set_thickness(3);
    seed_stroke(
        &mut editor,
        &[(0.0, 100.0), (4.0 * 240.0, 101.0), (8.0 * 240.0, 99.0)],
    );

    // 预览展开成 (track, tick_cell, key) 集合（集合语义：行段拆段/重叠不改变覆盖）
    let mut preview: Vec<(usize, i64, u16)> = Vec::new();
    for (track, key, t0, t1) in editor.brush_preview_runs() {
        for tick in t0..=t1 {
            preview.push((track, tick, key));
        }
    }
    preview.sort_unstable();
    let preview_set: std::collections::BTreeSet<(usize, i64, u16)> =
        preview.iter().copied().collect();

    let mut before: Vec<(usize, i64, u16)> = editor
        .brush_pending_notes()
        .iter()
        .map(|n| (n.track, n.tick_cell, n.key))
        .collect();
    before.sort_unstable();
    let before_set: std::collections::BTreeSet<(usize, i64, u16)> =
        before.iter().copied().collect();
    if preview_set != before_set {
        let only_preview: Vec<_> = preview_set.difference(&before_set).take(8).collect();
        let only_notes: Vec<_> = before_set.difference(&preview_set).take(8).collect();
        panic!(
            "预览覆盖集 != 待生成音符集\n仅预览有({}): {:?}\n仅音符有({}): {:?}",
            preview_set.difference(&before_set).count(),
            only_preview,
            before_set.difference(&preview_set).count(),
            only_notes
        );
    }
    assert_eq!(
        preview_set, before_set,
        "行段展开（集合语义）== 待生成音符项（同源）"
    );

    assert!(editor.confirm_brush());
    let mut generated: Vec<(usize, i64, u16)> = Vec::new();
    for track in 0..4 {
        for note in editor.editor_state.data.track_notes(track).iter() {
            generated.push((
                track,
                (note.start_tick as f32 / 240.0) as i64,
                note.key as u16,
            ));
        }
    }
    generated.sort_unstable();
    assert_eq!(
        generated, before,
        "★ 所见即生成：预览方块 == √ 实际生成的音符（逐格一致）"
    );
}

// ── 按钮定位 ─────────────────────────

#[test]
fn test_button_rects_inside_content_area() {
    let mut editor = brush_editor();
    seed_stroke(
        &mut editor,
        &[(0.0, VISIBLE_KEY), (1920.0, VISIBLE_KEY + 2.0)],
    );
    let btns = brush_button_rects(&editor).expect("有笔画应有一对按钮");
    let content = content_bounds(&editor);
    for rect in [btns.confirm, btns.cancel] {
        assert!(rect.x >= content.x, "按钮不得超出内容区左缘");
        assert!(rect.y >= content.y, "按钮不得超出内容区顶缘");
        assert!(rect.x + rect.width <= content.x + content.width);
        assert!(rect.y + rect.height <= content.y + content.height);
    }
    assert!(btns.cancel.x > btns.confirm.x, "只有一对按钮（√ 左 × 右）");
}

#[test]
fn test_button_rects_none_without_strokes_or_wrong_tool() {
    let mut editor = brush_editor();
    assert!(brush_button_rects(&editor).is_none(), "无笔画不显示");

    seed_stroke(&mut editor, &[(0.0, VISIBLE_KEY)]);
    assert!(brush_button_rects(&editor).is_some());

    editor.editor_state.tool = Tool::Pencil;
    assert!(brush_button_rects(&editor).is_none(), "非画刷不显示");
}
