//! 颜料桶填充集成测试：点击记录标记，√ 确认时按封闭图形内部生成音符
//!
//! 填充语义（基本矢量绘制软件模式）：点击封闭区域内部 → `line_tool.fill`
//! 记录**标记**；√ 确认时把闭环内部按**音高行区间**解析成音符——区间端点 =
//! 闭环边与音高行边界的交点，**不做任何网格量化**（因此断言的是"哪条音高行
//! 铺满了哪个 tick 区间"，不是格点数量）；× 清空；Ctrl+Z 撤销。

use super::*;
use crate::tests::test_helpers::seed_notes;
use lumino_core::Tool;
use lumino_midi_model::{ChunkedList, NoteEvent};

/// 音符集合里是否有覆盖 (tick, key) 的一条音符
fn covers(notes: &ChunkedList<NoteEvent>, tick: u32, key: u16) -> bool {
    notes
        .iter()
        .any(|n| n.key == key as u8 && n.start_tick <= tick && n.end_tick > tick)
}

/// 是否有一条恰好铺满 `[start, end)` 的音高行音符
fn fills_row(notes: &ChunkedList<NoteEvent>, start: u32, end: u32, key: u16) -> bool {
    notes
        .iter()
        .any(|n| n.key == key as u8 && n.start_tick == start && n.end_tick == end)
}

/// 构造封闭矩形（两条路径围成，snap 固定 480）：
/// P1: (0,60) → (960,60) → (960,62) → (0,62)（顶 + 右 + 底）
/// P2: (0,62) → (0,60)（左侧竖线）
///
/// 视图固定（zoom_x = 0.25、画布 800×600）→ 可见 tick 范围 [0, 2720] 覆盖整个
/// 矩形，填充不被可见范围裁剪。
fn rect_editor() -> Editor {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    editor.editor_state.view.snap_precision = 480.0;
    editor.editor_state.view.zoom_x = 0.25;
    editor.editor_state.view.zoom_y = 4.0;
    editor.editor_state.canvas.size_x = 800.0;
    editor.editor_state.canvas.size_y = 600.0;
    editor.editor_state.line_tool.fill_enabled = true;
    {
        let line = &mut editor.editor_state.line_tool;
        line.paths.push(Vec::new());
        line.push_anchor(0, (0.0, 60.0));
        line.push_anchor(0, (960.0, 60.0));
        line.push_anchor(0, (960.0, 62.0));
        line.push_anchor(0, (0.0, 62.0));
        line.paths.push(Vec::new());
        line.push_anchor(1, (0.0, 62.0));
        line.push_anchor(1, (0.0, 60.0));
    }
    editor
}

#[test]
fn test_fill_stores_mark_not_notes() {
    let mut editor = rect_editor();
    seed_notes(&mut editor, 2, 1, &[]);
    editor.handle_fill_pressed(Point::new(100.0, 100.0), 480.0, 61);
    // 点击只记录标记：不直接生成音符、不做区域计算
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        0,
        "填充不应直接生成音符"
    );
    assert_eq!(
        editor.editor_state.line_tool.fill,
        vec![(480.0f32, 61u16)],
        "fill = 点击标记（非预计算格点集）"
    );
}

#[test]
fn test_fill_undoable() {
    let mut editor = rect_editor();
    seed_notes(&mut editor, 2, 1, &[]);
    editor.handle_fill_pressed(Point::new(100.0, 100.0), 480.0, 61);
    assert!(editor.undo(), "填充操作可撤销");
    assert!(
        !editor.editor_state.line_tool.has_fill(),
        "撤销后填充标记清空"
    );
    assert!(editor.redo(), "可重做");
    assert!(editor.editor_state.line_tool.has_fill(), "重做后填充恢复");
}

#[test]
fn test_fill_click_again_clears() {
    let mut editor = rect_editor();
    seed_notes(&mut editor, 2, 1, &[]);
    editor.handle_fill_pressed(Point::new(100.0, 100.0), 480.0, 61);
    // 再点已标记格点 → 取消全部填充
    editor.handle_fill_pressed(Point::new(100.0, 100.0), 480.0, 61);
    assert!(
        !editor.editor_state.line_tool.has_fill(),
        "再次点击已填充区域取消填充"
    );
}

#[test]
fn test_fill_boundary_click_fills_side() {
    let mut editor = rect_editor();
    seed_notes(&mut editor, 2, 1, &[]);
    // 点击边界格点 (0,60)：标记记录（归属由 √ 时几何判定决定）
    editor.handle_fill_pressed(Point::new(0.0, 0.0), 0.0, 60);
    assert!(editor.editor_state.line_tool.has_fill());
    // 点击外部 (1440,61) → 背景蔓延标记，同样进入编辑层
    editor.handle_fill_pressed(Point::new(0.0, 0.0), 1440.0, 61);
    assert!(
        editor.editor_state.line_tool.has_fill(),
        "外部区域标记存入编辑层"
    );
}

#[test]
fn test_confirm_merges_path_and_fill() {
    let mut editor = rect_editor();
    seed_notes(&mut editor, 2, 1, &[]);
    editor.handle_fill_pressed(Point::new(100.0, 100.0), 480.0, 61);
    assert!(editor.confirm_line_tool());
    let notes = editor.editor_state.data.current_track_notes();
    // 轮廓（顶/底各铺满 + 两条竖直边各 1 tick）+ 填充（60/61/62 三行铺满）
    // → 去重后 6 条：三条铺满 [0,960) 的行音符 + 960 处三条 1 tick 音符
    assert_eq!(notes.len(), 6, "轮廓 + 填充合并去重: {notes:?}");
    for key in [60u16, 61, 62] {
        assert!(
            fills_row(notes, 0, 960, key),
            "音高行 {key} 被填充铺满（含只有填充能给的内部行 61）"
        );
    }
    assert!(
        editor.editor_state.line_tool.paths.is_empty() && !editor.editor_state.line_tool.has_fill(),
        "确认后清空路径与填充"
    );
}

#[test]
fn test_cancel_clears_fill() {
    let mut editor = rect_editor();
    seed_notes(&mut editor, 2, 1, &[]);
    editor.handle_fill_pressed(Point::new(100.0, 100.0), 480.0, 61);
    editor.cancel_line_tool();
    assert!(!editor.editor_state.line_tool.has_fill());
    assert!(
        !editor.editor_state.line_tool.can_undo_path(),
        "取消清空历史"
    );
    assert_eq!(editor.editor_state.data.current_track_note_count(), 0);
}

#[test]
fn test_fill_mode_press_does_not_create_path() {
    // fill_enabled = true 时 pressed 走填充，不创建曲线路径
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    editor.editor_state.line_tool.fill_enabled = true;
    seed_notes(&mut editor, 2, 1, &[]);
    editor.handle_pressed(Point::new(120.0, 24.0), false);
    assert!(
        editor.editor_state.line_tool.paths.is_empty(),
        "填充模式点击不创建路径"
    );
}

#[test]
fn test_fill_enabled_state_lives_on_line_tool() {
    // 开关状态持久于 line_tool（Editor::set_fill_enabled 读写）
    let mut editor = Editor::new();
    assert!(!editor.fill_enabled());
    editor.set_fill_enabled(true);
    assert!(editor.fill_enabled());
    // 切到非曲线工具自动关闭
    editor.editor_state.tool = Tool::Curve;
    editor.set_tool(Tool::Pointer);
    assert!(!editor.fill_enabled(), "切走曲线工具自动关闭填充");
}

#[test]
fn test_fill_full_ui_flow_default_snap() {
    // 真实 UI 链路（默认 snap=1920）：工具栏开关 → 画布点击（handle_pressed 入口）
    // → 标记存入编辑层（不生成音符）→ √ 确认按封闭图形内部生成实心音符
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    // 视图参数使矩形 (0,60)-(5760,62) 位于画布可见区域：
    // x = 1920*0.25 + 键盘宽 ≈ 540、y = (127-61)*4 + 标尺 ≈ 294，均在 800x600 内
    editor.editor_state.view.zoom_x = 0.25;
    editor.editor_state.view.zoom_y = 4.0;
    editor.editor_state.canvas.size_x = 800.0;
    editor.editor_state.canvas.size_y = 600.0;
    {
        let line = &mut editor.editor_state.line_tool;
        // 封闭矩形（1920 网格）：顶 + 右 + 底 + 左侧竖线
        line.paths.push(Vec::new());
        line.push_anchor(0, (0.0, 60.0));
        line.push_anchor(0, (5760.0, 60.0));
        line.push_anchor(0, (5760.0, 62.0));
        line.push_anchor(0, (0.0, 62.0));
        line.paths.push(Vec::new());
        line.push_anchor(1, (0.0, 62.0));
        line.push_anchor(1, (0.0, 60.0));
    }
    seed_notes(&mut editor, 2, 1, &[]);
    // 工具栏开启颜料桶（FillToggled → set_fill_enabled）
    editor.set_fill_enabled(true);
    assert!(editor.fill_enabled());
    // 画布点击矩形内部格点 (1920,61)（handle_pressed 完整入口：snap_tick floor）
    let p = editor.line_pos_screen_pos((1920.0, 61.0));
    editor.handle_pressed(p, false);
    assert_eq!(
        editor.editor_state.line_tool.fill.len(),
        1,
        "点击只记录一个标记"
    );
    assert_eq!(editor.editor_state.data.current_track_note_count(), 0);
    // √ 确认：轮廓 + 填充（矩形内部整体铺满）合并生成实心音符
    assert!(editor.confirm_line_tool());
    let notes = editor.editor_state.data.current_track_notes();
    for key in [60u16, 61, 62] {
        assert!(
            fills_row(notes, 0, 5760, key),
            "音高行 {key} 铺满整个封闭图形（不受可见范围裁剪）"
        );
    }
}

/// 弯曲封闭图形（左侧竖线带自定义柄向上拱起 → 采样跳格产生边界缝隙）
fn bent_rect_editor() -> Editor {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    editor.editor_state.view.snap_precision = 480.0;
    editor.editor_state.view.zoom_x = 0.25;
    editor.editor_state.view.zoom_y = 4.0;
    editor.editor_state.canvas.size_x = 800.0;
    editor.editor_state.canvas.size_y = 600.0;
    editor.editor_state.line_tool.fill_enabled = true;
    {
        let line = &mut editor.editor_state.line_tool;
        line.paths.push(Vec::new());
        line.push_anchor(0, (0.0, 60.0));
        line.push_anchor(0, (1920.0, 60.0));
        line.push_anchor(0, (1920.0, 62.0));
        line.push_anchor(0, (0.0, 62.0));
        line.paths.push(Vec::new());
        line.push_anchor(1, (0.0, 62.0));
        line.push_anchor(1, (0.0, 60.0));
    }
    // 左侧竖线弯曲拱起：采样格点 key 序列 62→67→71→73→…→60（跳格，
    // 修复前边界在 (0,61) 等位置有缝隙，泛洪双向漏穿）
    {
        let line = &mut editor.editor_state.line_tool;
        line.paths[1][0].set_out_handle((0.0, 16.0));
        line.paths[1][1].set_in_handle((0.0, 16.0));
    }
    editor
}

#[test]
fn test_fill_bent_curve_sealed_no_leak() {
    // 弯曲封闭图形内部可填：网格泛洪时代采样跳格导致漏穿
    // （表现为"封闭图形填不上、背景被填"）；几何绕数判定由曲线几何
    // 决定内部，缝隙从根上不存在 → √ 后轮廓内部整行铺满。
    let mut editor = bent_rect_editor();
    seed_notes(&mut editor, 2, 1, &[]);
    editor.handle_fill_pressed(Point::new(100.0, 100.0), 480.0, 61);
    assert_eq!(
        editor.editor_state.line_tool.fill,
        vec![(480.0f32, 61u16)],
        "点击记录标记"
    );
    assert!(editor.confirm_line_tool());
    let notes = editor.editor_state.data.current_track_notes();
    // 矩形 (0,60)-(1920,62)：内部 = 整宽 [0,1920)，每行一条音符
    for key in [60u16, 61] {
        assert!(
            fills_row(notes, 0, 1920, key),
            "弯曲封闭图形内部音高行 {key} 铺满（无缝隙、不漏穿）"
        );
    }
    assert!(!covers(notes, 0, 100), "不得到弧线上方外部（背景不能被填）");
}

#[test]
fn test_fill_all_bent_closed_shape_sealed() {
    // 纯弯曲闭合图形（顶弧 + 底弧，无直线段）：两弧采样均跳格，
    // 缺口链直接通向外部。修复前内部点击泛洪从弧线缺口漏穿
    // （"封闭图形填不上、背景被填"）；几何绕数判定从根上密封。
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    editor.editor_state.view.snap_precision = 1920.0;
    editor.editor_state.view.zoom_x = 0.25;
    editor.editor_state.view.zoom_y = 4.0;
    editor.editor_state.canvas.size_x = 800.0;
    editor.editor_state.canvas.size_y = 600.0;
    editor.editor_state.line_tool.fill_enabled = true;
    {
        let line = &mut editor.editor_state.line_tool;
        // 顶弧：(0,60) → (5760,60)，柄向上拱 30 → 采样跳格
        line.paths.push(Vec::new());
        line.push_anchor(0, (0.0, 60.0));
        line.push_anchor(0, (5760.0, 60.0));
        // 底弧：(5760,60) → (0,60)，柄向下沉 30
        line.paths.push(Vec::new());
        line.push_anchor(1, (5760.0, 60.0));
        line.push_anchor(1, (0.0, 60.0));
        // 自定义弯曲柄（push_anchor 之后设置，避免被自动重算覆盖）
        line.paths[0][0].set_out_handle((2880.0, 30.0));
        line.paths[0][1].set_in_handle((-2880.0, 30.0));
        line.paths[1][0].set_out_handle((-2880.0, -30.0));
        line.paths[1][1].set_in_handle((2880.0, -30.0));
    }
    // 点击两弧之间内部 (1920,61)
    seed_notes(&mut editor, 2, 1, &[]);
    editor.handle_fill_pressed(Point::new(100.0, 100.0), 1920.0, 61);
    assert_eq!(
        editor.editor_state.line_tool.fill,
        vec![(1920.0f32, 61u16)],
        "点击记录标记"
    );
    assert!(editor.confirm_line_tool(), "封闭图形内部可生成音符");
    let notes = editor.editor_state.data.current_track_notes();
    // 两弧之间（x=1920 处顶弧高约 78、底弧低约 42）完全铺满
    assert!(
        covers(notes, 1920, 61) && covers(notes, 1920, 75),
        "两弧之间内部完全铺满（不只填点击处）"
    );
    assert!(
        !covers(notes, 1920, 113),
        "不得填充到弧线上方外部（背景不能被填）"
    );
    assert!(!covers(notes, 1920, 20), "不得填充到弧线下方外部");
}

#[test]
fn test_fill_two_curves_nearly_closed() {
    // 用户场景：**两条弯曲曲线**组成封闭图形，右侧接缝差 1 key
    // （手画未精确闭合）。端点容差补边封口 → 内部可填、不蔓延背景。
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    editor.editor_state.view.snap_precision = 1920.0;
    editor.editor_state.view.zoom_x = 0.25;
    editor.editor_state.view.zoom_y = 4.0;
    editor.editor_state.canvas.size_x = 800.0;
    editor.editor_state.canvas.size_y = 600.0;
    editor.editor_state.line_tool.fill_enabled = true;
    {
        let line = &mut editor.editor_state.line_tool;
        // P1 顶弧：(0,60) → (5760,61)（终点差 1 key 未接上 P2 起点）
        line.paths.push(Vec::new());
        line.push_anchor(0, (0.0, 60.0));
        line.push_anchor(0, (5760.0, 61.0));
        line.paths[0][0].set_out_handle((2880.0, 30.0));
        line.paths[0][1].set_in_handle((-2880.0, 30.0));
        // P2 底弧：(5760,62) → (0,60)
        line.paths.push(Vec::new());
        line.push_anchor(1, (5760.0, 62.0));
        line.push_anchor(1, (0.0, 60.0));
        line.paths[1][0].set_out_handle((-2880.0, -30.0));
        line.paths[1][1].set_in_handle((2880.0, -30.0));
    }
    // 点击两弧之间内部 (1920,61)
    seed_notes(&mut editor, 2, 1, &[]);
    editor.handle_fill_pressed(Point::new(100.0, 100.0), 1920.0, 61);
    assert!(
        editor.confirm_line_tool(),
        "接缝差 1 key 的封闭图形内部可填"
    );
    let notes = editor.editor_state.data.current_track_notes();
    assert!(covers(notes, 1920, 61), "内部被填充");
    assert!(!covers(notes, 1920, 113), "不得蔓延到背景（弧线上方外部）");
}

// ── 切分档位（x 分音符填充） ──

#[test]
fn test_ctrl_click_requests_dialog_instead_of_marking() {
    // Ctrl+单击 = 请求打开分音符弹窗，不记填充标记
    let mut editor = rect_editor();
    seed_notes(&mut editor, 2, 1, &[]);
    editor.set_ctrl_pressed(true);
    editor.handle_fill_pressed(Point::new(100.0, 100.0), 480.0, 61);
    assert!(
        editor.editor_state.line_tool.fill.is_empty(),
        "Ctrl+单击不记填充标记"
    );
    assert!(
        editor.take_fill_division_dialog_request(),
        "Ctrl+单击应置位弹窗请求"
    );
    assert!(
        !editor.take_fill_division_dialog_request(),
        "请求标志是一次性的（取走后清零）"
    );
}

#[test]
fn test_plain_click_does_not_request_dialog() {
    let mut editor = rect_editor();
    seed_notes(&mut editor, 2, 1, &[]);
    editor.handle_fill_pressed(Point::new(100.0, 100.0), 480.0, 61);
    assert!(
        !editor.take_fill_division_dialog_request(),
        "普通单击不得弹窗"
    );
    assert_eq!(editor.editor_state.line_tool.fill.len(), 1);
}

#[test]
fn test_fill_division_splits_notes_on_confirm() {
    // 矩形 tick ∈ [0,960]；ppq = 480 → 四分音符档（480 tick）切出 2 条/行
    let mut editor = rect_editor();
    editor.editor_state.view.ppq = 480;
    seed_notes(&mut editor, 2, 1, &[]);
    editor.set_fill_division(Some(4));
    editor.handle_fill_pressed(Point::new(100.0, 100.0), 480.0, 61);
    assert!(editor.confirm_line_tool());
    let notes = editor.editor_state.data.current_track_notes();
    assert!(fills_row(notes, 0, 480, 61), "第一行四分音符: {notes:?}");
    assert!(fills_row(notes, 480, 960, 61), "第二行四分音符: {notes:?}");
    // 不得残留整条长音符（切分前的行为）
    assert!(
        !fills_row(notes, 0, 960, 61),
        "切分开启后不应再生成整块长音符"
    );
}

#[test]
fn test_fill_division_survives_confirm() {
    // 档位是填充桶的模式设置：√ 确认后保留（reset 显式保住）
    let mut editor = rect_editor();
    seed_notes(&mut editor, 2, 1, &[]);
    editor.set_fill_division(Some(16));
    editor.handle_fill_pressed(Point::new(100.0, 100.0), 480.0, 61);
    assert!(editor.confirm_line_tool());
    assert_eq!(
        editor.fill_division(),
        Some(16),
        "√ 确认后切分档位保留，避免每次重填"
    );
}

#[test]
fn test_fill_division_preview_matches_generated_notes() {
    // 不变式：预览（fill_region.blocks）== √ 生成的音符（fill_notes）
    let mut editor = rect_editor();
    editor.editor_state.view.ppq = 480;
    seed_notes(&mut editor, 2, 1, &[]);
    editor.set_fill_division(Some(8));
    editor.handle_fill_pressed(Point::new(100.0, 100.0), 480.0, 61);

    let preview = crate::interaction::line_tool::fill::region::fill_region(&editor)
        .expect("有填充 → 有预览区域");
    assert!(!preview.blocks.is_empty(), "切分档位下预览应给出音符块");
    let generated = crate::interaction::line_tool::fill::fill_notes(&editor);
    assert_eq!(
        preview.blocks.len(),
        generated.len(),
        "预览块数 == 生成音符数（同源计算）"
    );
    for (i, n) in generated.iter().enumerate() {
        assert_eq!(
            preview.blocks[i],
            (n.start as f32, n.end as f32, n.key as f32),
            "第 {i} 块预览与生成音符一致"
        );
    }
}

// ── 大批量填充 √ 确认的写入路径（渲染事件风暴回归）────────────────────

/// 「大批量」判定量级（固定字面量，**故意不引用 `BATCH_INSERT_THRESHOLD`**）：
/// 若引用阈值，一旦阈值被改动，前置断言会先失败，掩盖「写入路径是否真的批量」
/// 这条真正要守的断言（牙齿检查时实测过这个坑）。
const LARGE_FILL_NOTES: usize = 2048;

/// 构造「大面积填充」编辑器：88 音高行 × 24000 tick，切分 1/16 → ≈ 8800 音符
fn large_fill_editor() -> Editor {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Curve;
    editor.editor_state.view.snap_precision = 240.0;
    editor.editor_state.view.ppq = 960;
    editor.editor_state.view.key_count = 128;
    editor.editor_state.view.visible_key_count = 128;
    editor.editor_state.view.zoom_x = 0.02;
    editor.editor_state.view.zoom_y = 4.0;
    editor.editor_state.canvas.size_x = 8000.0;
    editor.editor_state.canvas.size_y = 1200.0;
    seed_notes(&mut editor, 2, 1, &[]);
    editor.editor_state.line_tool.fill_enabled = true;
    // 切分档位 1/16：步长 = 4·ppq/x = 4·960/16 = 240 tick
    editor.editor_state.line_tool.fill_division = Some(16);
    let line = &mut editor.editor_state.line_tool;
    line.paths.push(Vec::new());
    for (t, k) in [
        (0.0, 40.0),
        (24000.0, 40.0),
        (24000.0, 127.0),
        (0.0, 127.0),
        (0.0, 40.0),
    ] {
        line.push_anchor(0, (t, k));
    }
    editor
}

/// 回归 BUG：`confirm_line_tool` 曾无条件**逐音符** `insert_note_with_id`——
/// 每个音符记一条 `NoteDeltaEvent::InsertAt`，渲染侧逐条发 `NoteEvent::Insert`
/// 并在 GPU 内搬移其后的全部实例。填充的生成顺序（按音高行、行内按 tick，
/// 跨行 tick 回绕）使每条插入都落在列表中部，Σtail 超线性增长：
/// 实测 13288 音符 = 13288 条消息 + 6805 万实例搬移（≈1038 MB GPU 拷贝）
/// + 13288 次 submit，√ 瞬间卷帘 WGPU 卡顿（见 `ui_fill_confirm_bench`）。
///
/// 修复后：超过 `BATCH_INSERT_THRESHOLD` 走批量归并——`note_delta_events` 为空，
/// 只发一次主轨结构重建（`main_track_struct_dirty`）。
#[test]
fn test_large_fill_confirm_uses_batch_write_no_event_storm() {
    let mut editor = large_fill_editor();
    editor.handle_fill_pressed(Point::new(100.0, 100.0), 1200.0, 60);

    assert!(editor.confirm_line_tool(), "大面积填充应生成音符");
    let count = editor.editor_state.data.current_track_note_count();
    assert!(
        count > LARGE_FILL_NOTES,
        "前置：生成音符数应超过批量阈值，实际 {count}"
    );

    // 关键断言：不得出现逐音符 InsertAt 事件风暴
    assert!(
        editor.editor_state.data.note_delta_events.is_empty(),
        "大批量填充应走批量归并（0 条段内增量事件），实际 {} 条",
        editor.editor_state.data.note_delta_events.len()
    );
    assert!(
        editor.editor_state.data.main_track_struct_dirty,
        "大批量填充应以单次主轨结构重建（TrackDelta）同步渲染"
    );

    // 撤销语义：一次 √ = 一次历史记录，Ctrl+Z 全退
    assert!(editor.undo(), "填充结果应可撤销");
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        0,
        "撤销应移除本次填充的全部音符"
    );
}

/// 对照组：小规模填充仍走逐音符插入（保留 GPU 段内增量，避免整段重建）
#[test]
fn test_small_fill_confirm_keeps_per_note_incremental_events() {
    let mut editor = rect_editor();
    seed_notes(&mut editor, 2, 1, &[]);
    editor.handle_fill_pressed(Point::new(100.0, 100.0), 480.0, 61);

    assert!(editor.confirm_line_tool(), "小矩形填充应生成音符");
    let count = editor.editor_state.data.current_track_note_count();
    assert!(count > 0 && count <= LARGE_FILL_NOTES);
    assert!(
        !editor.editor_state.data.note_delta_events.is_empty(),
        "小规模填充应保留逐音符段内增量事件"
    );
}
