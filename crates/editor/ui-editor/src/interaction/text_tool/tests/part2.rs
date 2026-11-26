//! 由 `tests.rs` 拆出的后半段测试，按原顺序保留。

use super::*;

#[test]
fn test_text_tool_press_noop_on_conductor_track() {
    // 回归：文字工具在 Conductor 音轨（track 0）上「整个不可用」——
    // 即便完成「按下 → 释放」整段交互，也不得进入编辑态、不得生成任何音符。
    use crate::tests::test_helpers::seed_notes;
    use lumino_message::{EditorAction, Point2, Tool};

    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Text;
    // 选中 Conductor 音轨（track 0）
    seed_notes(&mut editor, 1, 0, &[]);

    // 模拟在 Conductor 轨上「按下 → 释放」整段交互
    editor.handle_action(EditorAction::Pressed {
        pos: Point2::new(100.0, 100.0),
        shift: false,
        ctrl: false,
    });
    editor.handle_action(EditorAction::Released);

    // 入口交互被拦截：文本框从未激活、从未进入编辑态
    assert!(!editor.text_tool_allowed(), "当前轨应为 Conductor");
    assert!(
        !editor.editor_state.text_tool.active,
        "Conductor 音轨上文字工具不得激活任何文本框"
    );
    assert!(!editor.editor_state.text_tool.editing);

    // released 处的 begin_editing 也被 Conductor 守卫拦下，不得置位激活状态
    assert!(!editor.editor_state.text_tool.active);

    // 文档侧：Conductor 轨不得写入任何音符
    assert_eq!(
        editor.editor_state.data.track_notes(0).len(),
        0,
        "Conductor 音轨不应写入任何音符"
    );
}

// ─────────────────── 卷帘方向 × 文字工具（轴角色 / 几何 / 生成）回归 ───────────────────

/// 构造文字工具编辑器（画布 800×600、键盘 120）。
///
/// 视图固定：`zoom_x = 1`、`zoom_y = 4`、无滚动、`snap = 480`、无标尺。
/// 纵向落点换算：`key = x/4`、`tick = 480 − y`（网格 y ∈ [0,480)）。
///
/// 纵向卷帘的文字工具编辑器
fn vertical_text_editor() -> Editor {
    text_tool_editor(true)
}

/// 横向轴角色（与纵向互为对调）：列→tick（+snap）、行→key（−1）。
///
/// 本用例锁死"轴角色互换"没有波及横向：横向仍是「列 = 时间格、行 = 音高格」，
/// 且字形 advance 沿屏幕向右、字形"向下"沿屏幕向下（key 递减 → Y 递增）。
#[test]
fn test_horizontal_glyph_grid_axes() {
    let mut editor = text_tool_editor(false);
    {
        let tt = &mut editor.editor_state.text_tool;
        // 框：tick [0, 960]（2 个 snap=480 时间格）× key [60, 64]（5 个音高格）
        tt.set_drag(0.0, 960.0, 60, 64);
        tt.active = true;
        tt.editing = true;
    }
    let snap = editor.editor_state.view.snap_precision;
    let grid = GlyphGrid::from_state(&editor.editor_state.text_tool, snap, false);
    assert_eq!(
        (grid.cols, grid.rows),
        (2, 5),
        "横向：列 = 时间格（tick 跨度 / snap）、行 = 音高格（key 跨度）"
    );
    // 行 0 = 最高 key（屏幕上方）；列 0 = 框左缘 tick
    assert_eq!(grid.cell(0, 0), (0.0, 64), "左上 = 起始 tick × 最高 key");
    assert_eq!(grid.cell(1, 0), (0.0, 63), "行 +1 = key 递减（屏幕向下）");
    assert_eq!(
        grid.cell(0, 1),
        (480.0, 64),
        "列 +1 = tick 前进一个精度单元"
    );

    let screen = |cell: (f32, u16)| editor.tick_key_to_pos(cell.0, cell.1);
    let c0 = screen(grid.cell(0, 0));
    let c1 = screen(grid.cell(0, 1));
    let r1 = screen(grid.cell(1, 0));
    assert!(c1.x > c0.x, "advance 沿屏幕向右：{c0:?} → {c1:?}");
    assert!((c1.y - c0.y).abs() < 0.01, "同一音高格内 Y 不应变化");
    assert!(r1.y > c0.y, "字形'向下'沿屏幕向下：{c0:?} → {r1:?}");
    assert!((r1.x - c0.x).abs() < 0.01, "同一时间格内 X 不应变化");
}

/// 纵向端到端回归（BUG：文字工具在纵向"能拉框、点了没反应"）。
///
/// **修复前**：`box_rect_screen` 在纵向直接 `return None`，而它是文本框、√×/模式按钮、
/// TextInput 覆盖层的唯一几何来源 → 三处同时消失，`confirm_text_tool` 无任何入口
/// （其唯一调用点是 `handle_text_tool_pressed` 的按钮命中分支），文字工具在纵向静默失效。
///
/// **修复后**整条链路必须走通：拉框 → 松手进入编辑态 → 框与三按钮存在 → 输入文字 →
/// 点 √ 生成落在框内（key ∈ 框的 key 范围）的音符。
#[test]
fn test_vertical_roll_text_tool_reaches_editing_and_confirms() {
    use crate::grid::text_tool_box::{box_rect_screen, button_rects};
    use lumino_message::EditorAction;

    let mut editor = vertical_text_editor();
    // 拉框：按下 (x=200 → key 50, y=400 → tick 80→吸附 0)，拖到 (x=280 → key 70, y=100 → tick 380→吸附 480)
    editor.handle_text_tool_pressed(Point::new(200.0, 400.0), 50);
    editor.handle_text_tool_moved(Point::new(280.0, 100.0));
    editor.handle_released();

    let tt = &editor.editor_state.text_tool;
    assert!(tt.active && tt.editing, "松手后应进入文字编辑态");
    assert_eq!(tt.normalized_ticks(), (0.0, 480.0), "tick 轴按精度吸附");
    assert_eq!(
        tt.normalized_keys(),
        (50, 70),
        "key 轴取拉框两端的屏幕 X 格"
    );

    // 几何存在性（修复前此处必为 None，是本回归的核心断言）
    let (l, t, r, b) = box_rect_screen(&editor).expect("纵向卷帘必须给出文本框几何");
    assert!(r > l && b > t, "框必须有面积：{l},{t},{r},{b}");
    let btns = button_rects(&editor).expect("纵向卷帘必须给出 √/×/模式 按钮");

    // 输入文字走真实消息路径（`set_text_tool_text` 要求 active）
    editor.handle_action(EditorAction::TextToolTextChanged("A".to_string()));
    assert_eq!(editor.editor_state.text_tool.text, "A");

    // 字体环境探测：无可用系统字体时只验几何（与既有测试的"CI 缺失则跳过"约定一致）
    let snap = editor.editor_state.view.snap_precision;
    let cols = editor.editor_state.text_tool.cols(snap);
    let rows = editor.editor_state.text_tool.rows();
    let family = editor.editor_state.text_tool.font_family;
    if legacy_occ("A", cols, rows, family).is_none() {
        eprintln!(
            "跳过 test_vertical_roll_text_tool_reaches_editing_and_confirms 的生成断言：无可用系统字体"
        );
        return;
    }

    // 点 √（按钮命中）→ 生成音符
    let center = Point::new(
        btns.confirm.x + btns.confirm.width * 0.5,
        btns.confirm.y + btns.confirm.height * 0.5,
    );
    editor.handle_text_tool_pressed(center, 50);
    assert!(
        !editor.editor_state.text_tool.active,
        "确认后文本框应被清空（进入下一轮）"
    );
    let notes = editor.editor_state.data.current_track_notes();
    assert!(!notes.is_empty(), "确认后当前轨应写入音符");
    for n in notes.iter() {
        assert!(
            (50..=70).contains(&n.key),
            "生成的音符必须落在框的 key 范围内，实际 key {}",
            n.key
        );
        assert!(
            n.start_tick <= 480,
            "生成的音符必须落在框的 tick 范围内，实际 start_tick {}",
            n.start_tick
        );
    }
}

/// 纵向卷帘：框内拖拽移动必须走 **key 轴（屏幕 X）** 平移，时间轴（Y）不动。
///
/// 横向的"框内按下"语义是 X=tick / Y=key；纵向转置后若仍按横向读取，拖动会退化为
/// "上下拖改 key、左右拖改 tick"——表象是框乱跳。此处钉死转置后的轴向。
#[test]
fn test_vertical_roll_text_box_drag_move_uses_key_axis() {
    let mut editor = vertical_text_editor();
    {
        let tt = &mut editor.editor_state.text_tool;
        tt.set_drag(0.0, 480.0, 50, 70);
        tt.active = true;
        tt.editing = true;
    }

    // 框内按下（x=260 → key 65，y=400 → tick 80）：应进入拖动
    editor.handle_text_tool_pressed(Point::new(260.0, 400.0), 65);
    assert!(
        editor.editor_state.text_tool.is_dragging(),
        "纵向框内按下也应进入拖拽移动"
    );

    // 向右拖到 x=300（key 75）：key 轴整体 +10 行，tick 不变
    editor.handle_text_tool_box_move(Point::new(300.0, 400.0));
    let tt = &editor.editor_state.text_tool;
    assert_eq!(
        (tt.start_key, tt.end_key),
        (60, 80),
        "右侧拖拽 = key 轴整体右移 10 行"
    );
    assert_eq!(
        (tt.start_tick, tt.end_tick),
        (0.0, 480.0),
        "时间轴未动则 tick 必须不变"
    );

    editor.handle_released();
    assert!(
        !editor.editor_state.text_tool.is_dragging(),
        "释放后应清除拖拽临时状态"
    );
}

/// 「所见即生成」的**生成 + 预览同格**证明（纵向），并锁死"正着读"的轴角色。
///
/// 纵向轴角色（`GlyphGrid`）：列→key（+1，屏幕向右）、行→tick（−snap，屏幕向下）。
/// 因此预览位图 x = 屏幕 X、位图 y = 屏幕 Y，**原样铺进框矩形即正立可读**，
/// 且每个墨格对应的那枚音符，其屏幕矩形必须与位图格**严格重合**。
#[test]
fn test_vertical_glyph_grid_matches_preview_and_notes() {
    use crate::grid::text_tool_box::box_rect_screen;

    let mut editor = vertical_text_editor();
    {
        let tt = &mut editor.editor_state.text_tool;
        // 框：tick [0, 960]（2 个 snap=480 时间格）× key [60, 64]（5 个音高格）
        tt.set_drag(0.0, 960.0, 60, 64);
        tt.active = true;
        tt.editing = true;
    }
    let snap = editor.editor_state.view.snap_precision;
    assert_eq!(snap, 480.0, "本测试的格数断言以 480 精度为前提");
    let grid = GlyphGrid::from_state(&editor.editor_state.text_tool, snap, true);
    assert_eq!(
        (grid.cols, grid.rows),
        (5, 2),
        "纵向：列 = 音高格（key 跨度）、行 = 时间格（tick 跨度 / snap）"
    );

    // 占用网格：行 0 的列 0/4、行 1 的列 4 有墨
    let occ = vec![
        vec![true, false, false, false, true],
        vec![false, false, false, false, true],
    ];
    let notes = sample_to_notes_grid(&occ, &grid, snap, false);
    assert_eq!(notes.len(), 3, "正常模式：每个墨格一条音符");
    assert!(
        notes.iter().all(|&(_, _, len)| len == snap),
        "正常模式音符长度恒为 snap"
    );
    // 行 0 占框顶那一格 ⇒ tick = 960 − 480 = 480；行 1 ⇒ tick = 0
    assert!(
        notes.contains(&(480.0, 60, snap)),
        "行 0 列 0 = 框顶 × 最低 key"
    );
    assert!(
        notes.contains(&(480.0, 64, snap)),
        "行 0 列 4 = 框顶 × 最高 key（advance 向右 = key 升高）"
    );
    assert!(
        notes.contains(&(0.0, 64, snap)),
        "行 1 列 4 = 框底 × 最高 key"
    );

    // 预览位图铺进框矩形后的屏幕格 vs 音符的屏幕矩形：逐格重合
    let (left, top, right, bottom) = box_rect_screen(&editor).expect("纵向应有框");
    let cell_w = (right - left) / grid.cols as f32;
    let cell_h = (bottom - top) / grid.rows as f32;
    for (row, row_ink) in occ.iter().enumerate() {
        for (col, &on) in row_ink.iter().enumerate() {
            if !on {
                continue;
            }
            let (tick, key) = grid.cell(row, col);
            // 位图格 (x=col, y=row) 的屏幕矩形
            let bx0 = left + col as f32 * cell_w;
            let by0 = top + row as f32 * cell_h;
            // 音符矩形：起点 tick（时间上最早 = 屏幕下方）→ 起点+snap（上方）
            let note_low = editor.tick_key_to_pos(tick, key);
            let note_high = editor.tick_key_to_pos(tick + snap, key);
            assert!(
                (note_low.x - bx0).abs() < 0.01,
                "音符格的屏幕左缘应等于位图格左缘（cell {row},{col}）"
            );
            assert!(
                (note_low.y - (by0 + cell_h)).abs() < 0.01,
                "音符起始 tick 应落在位图格下缘（纵向时间向上，cell {row},{col}）"
            );
            assert!(
                (note_high.y - by0).abs() < 0.01,
                "音符末端 tick 应落在位图格上缘（cell {row},{col}）"
            );
        }
    }
}

/// 纵向「key 范围合并」必须**沿时间轴（行方向）合并**。
///
/// 若照搬横向的"按行扫描合并"，纵向的合并方向会变成音高轴，于是"音高跨度"被当成
/// "音符长度"写入文档（长度单位错乱）。本测试钉死：同一 key 列的连续时间格 → 一条
/// 长音符，长度 = 格数 × snap，起点 = 段内最小 tick。
#[test]
fn test_vertical_merged_merges_along_time_axis() {
    let mut editor = vertical_text_editor();
    {
        let tt = &mut editor.editor_state.text_tool;
        tt.set_drag(0.0, 960.0, 60, 64);
        tt.active = true;
        tt.editing = true;
    }
    let snap = editor.editor_state.view.snap_precision;
    let grid = GlyphGrid::from_state(&editor.editor_state.text_tool, snap, true);
    assert_eq!((grid.cols, grid.rows), (5, 2));

    // 列 2（key 62）两行都有墨 → 合并为一条长音符（起点 0 = 段内最小 tick）
    let occ = vec![
        vec![false, false, true, false, false],
        vec![false, false, true, false, false],
    ];
    let notes = sample_to_notes_grid(&occ, &grid, snap, true);
    assert_eq!(
        notes,
        vec![(0.0, 62, 2.0 * snap)],
        "同一 key 列的连续时间格应合并为一条长音符（长度 = 时间跨度）"
    );

    // 空隙必须断开：列 2 只有行 0 有墨 → 单格音符
    let occ_gap = vec![
        vec![false, false, true, false, false],
        vec![false, false, false, false, false],
    ];
    assert_eq!(
        sample_to_notes_grid(&occ_gap, &grid, snap, true),
        vec![(480.0, 62, snap)],
        "行 0 单独有墨 = 框顶那一格（tick 480）"
    );
}
