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

// ── ★ 回归：可见窗口必须用**画布局部坐标**（bounds.position 是窗口坐标） ────────
//
// BUG：`Program::draw` 的 bounds.position 是画布组件在窗口里的偏移
// （左侧栏宽 + 音轨列表、工具栏高 + 标题栏），而帧内绘制坐标是画布局部坐标
// （iced 在调用前 `with_translation(bounds.x, bounds.y)`）。旧实现把 bounds 当
// 局部矩形用 → 可见窗口整体右移 offset_x、下移 offset_y → 笔画左上角被裁掉。

/// 真实布局偏移：左侧栏 240px + 标题栏 30px + 工具栏 54px = (240, 84)
const WIDGET_OFFSET: (f32, f32) = (240.0, 84.0);
/// 画布尺寸（= bounds.size()）
const CANVAS_SIZE: (f32, f32) = (1200.0, 800.0);

/// 构造"有真实布局偏移"的编辑器：键盘列 120、标尺 24、zoom 0.1/20、精度 240
fn window_editor() -> Editor {
    let mut editor = brush_editor();
    editor.brush.set_thickness(1);
    editor.editor_state.canvas.size_x = CANVAS_SIZE.0;
    editor.editor_state.canvas.size_y = CANVAS_SIZE.1;
    let view = &mut editor.editor_state.view;
    view.zoom_x = 0.1;
    view.zoom_y = 20.0;
    view.keyboard_width = 120.0;
    view.ruler_height = 24.0;
    view.visible_key_count = 128;
    view.scroll_x = 0.0;
    view.scroll_y = 0.0;
    editor
}

fn bounds_at(offset: (f32, f32)) -> Rectangle {
    Rectangle::new(
        Point::new(offset.0, offset.1),
        Size::new(CANVAS_SIZE.0, CANVAS_SIZE.1),
    )
}

fn local_canvas_bounds() -> Rectangle {
    Rectangle::new(
        Point::new(0.0, 0.0),
        Size::new(CANVAS_SIZE.0, CANVAS_SIZE.1),
    )
}

#[test]
fn test_visible_window_ignores_widget_offset() {
    let editor = window_editor();
    let zero = brush_visible_window(&editor, bounds_at((0.0, 0.0)));
    let offset = brush_visible_window(&editor, bounds_at(WIDGET_OFFSET));
    assert_eq!(
        zero, offset,
        "可见窗口只由画布尺寸决定；bounds.position（窗口坐标）一旦参与计算就是错位"
    );
    assert_eq!(
        zero,
        cov::CellWindow {
            cell_lo: -6,
            cell_hi: 46,
            key_lo: 87,
            key_hi: 128
        },
        "窗口 = 局部 (0,0)-(1200,800) 映射出的逻辑区间（含 1 格余量）"
    );
    // 内容区第一行 key=127（y ∈ [24, 44)）、tick 0 起始格都必须在窗口内
    assert!(zero.key_hi >= 127, "顶部可见行必须保留");
    assert!(zero.cell_lo <= 0, "tick 0 起始格必须保留");
}

#[test]
fn test_old_window_from_widget_offset_proves_clipping() {
    // 用**旧公式**（把 bounds.position 当局部坐标）算一遍，量化被误剔除的区域——
    // 这个测试就是"裁剪区域错位"的证明：旧窗口顶边/左边正好落在窗口偏移处。
    let editor = window_editor();
    let snap = editor.editor_state.view.snap_precision.max(1.0);
    let bounds = bounds_at(WIDGET_OFFSET);

    let old_corner = Point::new(bounds.x, bounds.y);
    let old_key_hi = editor.pos_to_key(old_corner);
    let old_cell_lo = (editor.pos_to_tick(old_corner) / snap).floor() as i64 - 1;

    let top_key = editor.pos_to_key(Point::new(0.0, 0.0));
    let new = brush_visible_window(&editor, bounds);

    // 顶部：旧窗口顶边 = 局部 y=84px 处 → 落在 key 124，而可见首行是 key 127
    assert_eq!(top_key, 127, "画布局部 y=0 对应最顶可见 key");
    assert_eq!(old_key_hi, 124, "旧窗口顶边落在 key 124");
    assert_eq!(
        top_key - old_key_hi,
        3,
        "★ 顶部 3 个 KEY 被误剔除（offset_y 84px ÷ zoom_y 20px）"
    );
    assert!(new.key_hi >= top_key, "修复后顶部可见行保留");

    // 左侧：旧窗口左边 = 局部 x=240px（= offset_x）→ 左侧 [键盘宽, 240) 被误剔除
    assert_eq!(old_cell_lo, 4, "旧窗口左边落在第 4 格（x=240px）");
    assert!(
        new.cell_lo < 0,
        "修复后 tick 0 之前的格保留，越界由内容区裁剪负责"
    );
}

#[test]
fn test_preview_blocks_visible_at_top_left_with_widget_offset() {
    // 一笔横穿画布顶部第一行、从 tick 0 开始：旧实现整段被剔除（= 笔画被裁剪）
    let mut editor = window_editor();
    seed_stroke(&mut editor, &[(0.0, 127.0), (24.0 * 240.0, 127.0)]);
    let bounds = bounds_at(WIDGET_OFFSET);
    let window = brush_visible_window(&editor, bounds);
    let runs = editor.brush_preview_runs();
    assert_eq!(runs, vec![(1, 127, 0, 24)]);

    let rect = brush_run_screen_rect(&editor, &window, local_canvas_bounds(), runs[0])
        .expect("★ 顶部第一行的方块必须可见（旧实现被误剔除）");
    assert!((rect.y - 24.0).abs() < 1e-3, "行顶 = 标尺下方第一行，y=24");
    assert!((rect.x - 120.0).abs() < 1e-3, "tick 0 → x=键盘宽 120");
    assert!((rect.width - 600.0).abs() < 1e-3, "25 格 × 24px");
    assert!((rect.height - 20.0).abs() < 1e-3, "1 key 高");
}

#[test]
fn test_preview_is_clipped_to_roll_content_area() {
    // 键盘列 / 标尺带内不绘制：音符被键盘、标尺覆盖层遮住，预览必须同样被遮住。
    // 场景：视图右滚 scroll_x=36 → tick 0 的格投影到 x=84（落在键盘列内）。
    let mut editor = window_editor();
    editor.editor_state.view.scroll_x = 36.0;
    let bounds = bounds_at(WIDGET_OFFSET);
    let window = brush_visible_window(&editor, bounds);
    let local = local_canvas_bounds();

    // 完全落在键盘列内（x ∈ [84, 108)）→ 不绘制
    seed_stroke(&mut editor, &[(120.0, 100.0)]);
    let inside_keyboard = editor.brush_preview_runs();
    assert_eq!(inside_keyboard, vec![(1, 100, 0, 0)]);
    assert!(
        brush_run_screen_rect(&editor, &window, local, inside_keyboard[0]).is_none(),
        "键盘列内的方块不得绘制"
    );

    // 跨越键盘右缘（x ∈ [84, 132)）→ 裁剪到内容区左缘，只留 [120, 132)
    let mut editor = window_editor();
    editor.editor_state.view.scroll_x = 36.0;
    seed_stroke(&mut editor, &[(120.0, 100.0), (360.0, 100.0)]);
    let window = brush_visible_window(&editor, bounds);
    let straddling = editor.brush_preview_runs();
    assert_eq!(straddling, vec![(1, 100, 0, 1)]);
    let rect = brush_run_screen_rect(&editor, &window, local, straddling[0])
        .expect("跨键盘右缘的方块应保留裁剪后的部分");
    assert!((rect.x - 120.0).abs() < 1e-3, "左缘裁剪到键盘右边 120");
    assert!((rect.width - 12.0).abs() < 1e-3, "只保留键盘右侧可见部分");
}

#[test]
fn test_negative_tick_cells_are_not_ghost_notes() {
    // 纵深防御回归：负 tick 格（仅在绕过输入守卫直接构造笔画时才可能出现）不得
    // 参与预览或生成——`NoteEvent.start_tick` 是 u32，`(-240.0) as u32` 饱和成 0，
    // 否则"看不见的笔画"会在 tick 0 凭空生成幽灵音符。
    // 真实输入不可达：`handle_pressed` 的 `is_inside_canvas` 已拒绝键盘列落笔。
    let mut editor = window_editor();
    seed_stroke(&mut editor, &[(-240.0, 100.0), (-120.0, 100.0)]);
    assert!(
        editor.brush_preview_runs().is_empty(),
        "负 tick 格不参与预览"
    );
    assert!(
        editor.brush_pending_notes().is_empty(),
        "负 tick 格不生成音符（与预览同源）"
    );
    assert!(!editor.confirm_brush(), "无可生成音符时 √ 不产生历史记录");
}

#[test]
fn test_visible_window_vertical_ignores_widget_offset() {
    let mut editor = window_editor();
    editor.editor_state.is_vertical_roll = true;
    let zero = brush_visible_window(&editor, bounds_at((0.0, 0.0)));
    let offset = brush_visible_window(&editor, bounds_at(WIDGET_OFFSET));
    assert_eq!(zero, offset, "纵向同样只认画布尺寸");
    // 纵向：tick 沿 Y（越大越靠上，顶点在底部键盘上沿）、key 沿 X
    assert_eq!(
        zero,
        cov::CellWindow {
            cell_lo: -6,
            cell_hi: 30,
            key_lo: 0,
            key_hi: 61
        },
        "纵向窗口（含 1 格余量）"
    );
    assert!(zero.cell_hi >= 28, "最高可见 tick 格必须保留");
    assert_eq!(zero.key_lo, 0, "key 0 起即可见");
}

// ── ★ 窗口化预览（§17 长笔画掉帧修复）：语义等价 + 所见即生成 ────────────────

/// 行段 → `(音轨, key, 格)` 集合（集合语义：拆段/边界重复不影响覆盖）
fn runs_to_cells(runs: &[(usize, u16, i64, i64)]) -> std::collections::BTreeSet<(usize, u16, i64)> {
    let mut out = std::collections::BTreeSet::new();
    for (track, key, t_start, t_end) in runs {
        for tick in *t_start..=*t_end {
            out.insert((*track, *key, tick));
        }
    }
    out
}

#[test]
fn test_windowed_preview_equals_full_preview_inside_window() {
    // 窗口化的唯一风险是"丢格/造格"：必须严格等于"全量覆盖 ∩ 窗口"
    let mut editor = window_editor();
    editor.brush.set_thickness(3);
    // 一笔远超视口的长笔画（横跨 120 格）+ 一笔视口内短笔画
    seed_stroke(&mut editor, &[(0.0, 100.0), (120.0 * 240.0, 104.0)]);
    seed_stroke(&mut editor, &[(2.0 * 240.0, 96.0), (5.0 * 240.0, 96.0)]);

    let window = brush_visible_window(&editor, local_canvas_bounds());
    let full = runs_to_cells(&editor.brush_preview_runs());
    let inside = runs_to_cells(&editor.brush_preview_runs_in_window(window));

    assert!(inside.is_subset(&full), "窗口化不得凭空造格");
    let expect: std::collections::BTreeSet<(usize, u16, i64)> = full
        .iter()
        .copied()
        .filter(|(_, key, cell)| window.contains(*cell, *key))
        .collect();
    let missing: Vec<_> = expect.difference(&inside).take(8).collect();
    let extra: Vec<_> = inside.difference(&expect).take(8).collect();
    assert!(
        inside == expect,
        "窗口内一格不少、窗口外一格不多（与全量口径严格等价）\n缺 {}: {:?}\n多 {}: {:?}",
        expect.difference(&inside).count(),
        missing,
        inside.difference(&expect).count(),
        extra
    );
    assert!(
        !full.is_empty() && inside.len() < full.len(),
        "回归前提：长笔画必须真的被视口裁掉一部分（full {} vs inside {}）",
        full.len(),
        inside.len()
    );
}

#[test]
fn test_windowed_preview_skips_strokes_outside_window() {
    // 视口外整笔 → 一个行段都不产生（成本 O(段数)，这是长笔画每帧成本压平的关键）
    let mut editor = window_editor();
    seed_stroke(
        &mut editor,
        &[(500.0 * 240.0, 100.0), (600.0 * 240.0, 100.0)],
    );
    assert!(!editor.brush_preview_runs().is_empty(), "全量口径仍有行段");
    let window = brush_visible_window(&editor, local_canvas_bounds());
    assert!(
        editor.brush_preview_runs_in_window(window).is_empty(),
        "视口外的笔画不得产生任何行段"
    );
}

#[test]
fn test_windowed_preview_matches_generated_notes_inside_window() {
    // 窗口化以后仍然"所见即生成"：视口内预览格 == 视口内生成的音符
    let mut editor = window_editor();
    seed_stroke(&mut editor, &[(0.0, 100.0), (60.0 * 240.0, 100.0)]);
    let window = brush_visible_window(&editor, local_canvas_bounds());
    let preview_inside = runs_to_cells(&editor.brush_preview_runs_in_window(window));

    assert!(editor.confirm_brush());
    let mut generated: std::collections::BTreeSet<(usize, u16, i64)> =
        std::collections::BTreeSet::new();
    for track in 0..4 {
        for note in editor.editor_state.data.track_notes(track).iter() {
            generated.insert((
                track,
                note.key as u16,
                (note.start_tick as f32 / 240.0) as i64,
            ));
        }
    }
    let expect: std::collections::BTreeSet<(usize, u16, i64)> = generated
        .iter()
        .copied()
        .filter(|(_, key, cell)| window.contains(*cell, *key))
        .collect();
    assert_eq!(
        preview_inside, expect,
        "★ 视口内所见 == 视口内生成（窗口化不破坏所见即生成）"
    );
}

// ── ★ §18：预览改走 wgpu 实例（长笔画掉帧修复）之后的等价性 ────────────────

/// 实例 `(tick, key, length)` 展开成 `(key, 格)` 集合（实例 = 半开 tick 区间 × 1 key）
fn instances_to_cells(
    instances: &[(f32, u8, f32, [f32; 4])],
    snap: f32,
) -> std::collections::BTreeSet<(u16, i64)> {
    let mut out = std::collections::BTreeSet::new();
    for (tick, key, length, _color) in instances {
        let start = (tick / snap) as i64;
        let end = ((tick + length) / snap) as i64;
        for cell in start..end {
            out.insert((*key as u16, cell));
        }
    }
    out
}

#[test]
fn test_preview_instances_match_visible_generated_notes() {
    // ★ wgpu 预览实例（新渲染通路）必须与 √ 生成的音符在**视口内逐格一致**：
    // 这是"所见即生成"从几何一致升级到"同一个着色器"之后的最终口径。
    let mut editor = window_editor();
    editor.brush.set_thickness(3);
    seed_stroke(&mut editor, &[(0.0, 100.0), (60.0 * 240.0, 102.0)]);

    let window = brush_visible_window(&editor, local_canvas_bounds());
    let instances = editor.brush_preview_note_instances();
    assert!(!instances.is_empty(), "待确认笔画必须产出预览实例");
    let preview_cells = instances_to_cells(&instances, 240.0);

    assert!(editor.confirm_brush());
    let mut generated: std::collections::BTreeSet<(u16, i64)> = std::collections::BTreeSet::new();
    for track in 0..4 {
        for note in editor.editor_state.data.track_notes(track).iter() {
            let cell = (note.start_tick as f32 / 240.0) as i64;
            if window.contains(cell, note.key as u16) {
                generated.insert((note.key as u16, cell));
            }
        }
    }
    assert_eq!(
        preview_cells, generated,
        "★ 预览实例覆盖 == 视口内生成的音符（逐格一致）"
    );
}

#[test]
fn test_preview_instances_are_viewport_bounded() {
    // 实例数只与视口内行段相关：视口外的长笔画不产生实例（每帧成本与笔画长度解耦）
    let mut editor = window_editor();
    seed_stroke(
        &mut editor,
        &[(500.0 * 240.0, 100.0), (600.0 * 240.0, 100.0)],
    );
    assert!(
        editor.brush_preview_note_instances().is_empty(),
        "视口外的笔画不得产生预览实例"
    );

    // 视口内一笔 → 实例数 == 可见行段数（每段 1 个实例，连续格合并成长矩形）
    let mut editor = window_editor();
    seed_stroke(&mut editor, &[(0.0, 100.0), (20.0 * 240.0, 100.0)]);
    let window = brush_visible_window(&editor, local_canvas_bounds());
    let runs = editor.brush_preview_runs_in_window(window);
    let instances = editor.brush_preview_note_instances();
    assert_eq!(runs, vec![(1, 100, 0, 20)], "水平笔画合并成 1 条行段");
    assert_eq!(
        instances.len(),
        runs.len(),
        "每个可见行段恰好 1 个实例（连续同色格合并成一条矩形）"
    );
    let (tick, key, length, _color) = instances[0];
    assert_eq!(key, 100);
    assert!((tick - 0.0).abs() < 1e-3 && (length - 21.0 * 240.0).abs() < 1e-3);
}

#[test]
fn test_preview_instances_available_in_vertical_roll() {
    // 纵向卷帘复用同一条音符预览通路（gfx 侧 `note.draw_vertical`），
    // 因此实例数据必须与横向同样产出（旧 canvas 图层曾整体漏挂，见 §16.4）。
    // 纵向 key 沿 X：zoom_y=20、画布宽 1200 → 可见 key 0..=61，故取 key 30。
    let mut editor = window_editor();
    editor.editor_state.is_vertical_roll = true;
    seed_stroke(&mut editor, &[(0.0, 30.0), (10.0 * 240.0, 30.0)]);
    let instances = editor.brush_preview_note_instances();
    assert!(
        !instances.is_empty(),
        "纵向卷帘下待确认笔画同样产出预览实例"
    );
}
