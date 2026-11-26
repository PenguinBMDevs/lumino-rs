use super::*;
use crate::{EditState, Editor};
use lumino_ui_core::geom::Point;

fn text_tool_editor(is_vertical: bool) -> Editor {
    use crate::tests::test_helpers::seed_notes;
    use lumino_message::Tool;

    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Text;
    // 文字工具在 Conductor（track 0）整工具不可用：交互测试必须落在普通轨
    seed_notes(&mut editor, 2, 1, &[]);
    editor.editor_state.is_vertical_roll = is_vertical;
    editor.editor_state.canvas.size_x = 800.0;
    editor.editor_state.canvas.size_y = 600.0;
    {
        let view = &mut editor.editor_state.view;
        view.zoom_x = 1.0;
        view.zoom_y = 4.0;
        view.scroll_x = 0.0;
        view.scroll_y = 0.0;
        view.keyboard_width = 120.0;
        view.ruler_height = 0.0;
        view.snap_precision = 480.0;
        view.visible_key_count = 128;
    }
    editor
}

mod aspect;
mod cjk_letterbox;
mod preview_cache;

fn bundled_font() -> ab_glyph::FontArc {
    let bytes = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../resources/fonts/NotoSansSC-Regular.ttf"
    ));
    ab_glyph::FontArc::try_from_vec(bytes.to_vec()).expect("仓库内置 Noto Sans SC 应可解析")
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// 旧兼容路径的占用网格（`4×4` 方格 + 面积阈值）
///
/// 本文件的历史用例都建立在旧口径上：探测系统字体是否可用、断言旧「填满框」行为。
/// UI-023 之后新路径按屏幕比例采样，这些用例必须显式声明走 legacy。
fn legacy_occ(text: &str, cols: usize, rows: usize, family: &str) -> Option<Vec<Vec<bool>>> {
    rasterize_text(
        text,
        cols,
        rows,
        family,
        &RasterScale::legacy(false),
        lumino_core::TextAlign::Left,
    )
}

#[test]
fn test_legacy_default_golden_pixels() {
    let font = bundled_font();
    let (w, h, buf) =
        rasterize_glyph_alpha_with_font("LUMINO", 32, 16, &font).expect("内置字体应能栅格化 ASCII");
    assert_eq!((w, h), (128, 64), "SS=4 时 32×16 格应为 128×64 像素");
    let hash = fnv1a(&buf);
    assert_eq!(
        hash, 0xc8c3_24a0_eadc_3693,
        "LegacyBox 默认像素 golden 变化；实际 hash = {hash:#x}"
    );
}

#[test]
fn test_legacy_dispatch_matches_old_rasterizer() {
    let tt = lumino_editor_state::text_tool::TextToolState::new();
    let grid = GlyphGrid {
        cols: 32,
        rows: 16,
        vertical: false,
        tick_origin: 0.0,
        key_origin: 64,
        tick_step: 1920.0,
    };
    let scale = RasterScale::legacy(false);
    // 旧「填满框 + 横向拉伸」的冻结实现
    let old = rasterize_glyph_alpha(
        "LUMINO",
        grid.cols,
        grid.rows,
        &tt.params.font,
        &scale,
        tt.params.align,
    );
    let new = rasterize_glyph_alpha_for_tool(&tt, "LUMINO", &grid, &scale);
    assert_eq!(old, new, "LegacyBox 分派必须逐字节等同旧入口");
}

/// 横向卷帘的采样网格（**测试用**手动构造：这些用例喂合成占用网格，
/// 不经过文本框状态，故直接给出 dims / 原点，与 `GlyphGrid::from_state` 的横向分支同口径）。
fn horiz_grid(rows: usize, cols: usize, tick_lo: f32, key_top: u16, snap: f32) -> GlyphGrid {
    GlyphGrid {
        cols,
        rows,
        vertical: false,
        tick_origin: tick_lo,
        key_origin: key_top,
        tick_step: snap,
    }
}

#[test]
fn test_text_params_update_enters_parametric_and_can_revert() {
    let mut editor = Editor::new();
    let params = lumino_core::TextParams {
        threshold: 72.0,
        ..lumino_core::TextParams::default()
    };
    editor.set_text_params(params.clone());
    let tt = &editor.editor_state.text_tool;
    assert_eq!(tt.params, params);
    assert_eq!(
        tt.layout_mode,
        lumino_core::TextLayoutMode::Parametric,
        "改参数必须自动进入 Parametric"
    );

    editor.set_text_layout_mode(lumino_core::TextLayoutMode::LegacyBox);
    assert_eq!(
        editor.editor_state.text_tool.layout_mode,
        lumino_core::TextLayoutMode::LegacyBox
    );
    assert_eq!(
        editor.editor_state.text_tool.params, params,
        "切回兼容模式不得丢参数"
    );
}

#[test]
fn test_sample_to_notes_normal_single_cell() {
    let occ = vec![vec![true]];
    let grid = horiz_grid(1, 1, 0.0, 64, 1920.0);
    let notes = sample_to_notes_grid(&occ, &grid, 1920.0, false);
    assert_eq!(notes, vec![(0.0, 64, 1920.0)]);
}

#[test]
fn test_sample_to_notes_normal_grid() {
    // 2x2 全占用：正常模式生成 4 个独立音符，长度均为 snap
    let occ = vec![vec![true, true], vec![true, true]];
    let grid = horiz_grid(2, 2, 100.0, 70, 480.0);
    let notes = sample_to_notes_grid(&occ, &grid, 480.0, false);
    // 行 0 → key 70，行 1 → key 69
    assert_eq!(notes.len(), 4);
    assert!(notes.contains(&(100.0, 70, 480.0)));
    assert!(notes.contains(&(580.0, 70, 480.0)));
    assert!(notes.contains(&(100.0, 69, 480.0)));
    assert!(notes.contains(&(580.0, 69, 480.0)));
}

#[test]
fn test_sample_to_notes_merged_runs() {
    // 一行 [T, F, T, T, F]：合并为两个音符（[0],[2,3]）
    let occ = vec![vec![true, false, true, true, false]];
    let grid = horiz_grid(1, 5, 0.0, 60, 1920.0);
    let notes = sample_to_notes_grid(&occ, &grid, 1920.0, true);
    assert_eq!(notes, vec![(0.0, 60, 1920.0), (3840.0, 60, 3840.0)]);
}

#[test]
fn test_sample_to_notes_merged_gap_breaks() {
    // 相邻但有空隙的行：空隙处必须断开（不合并本应分开的笔画）
    let occ = vec![vec![true, false, true]];
    let grid = horiz_grid(1, 3, 0.0, 60, 100.0);
    let notes = sample_to_notes_grid(&occ, &grid, 100.0, true);
    // [0..0] len 100, [2..2] len 100
    assert_eq!(notes, vec![(0.0, 60, 100.0), (200.0, 60, 100.0)]);
}

#[test]
fn test_sample_to_notes_merged_multiline_keys() {
    // 两行：key_top=65 → 行 0 = 65，行 1 = 64
    let occ = vec![vec![true, true], vec![true, false]];
    let grid = horiz_grid(2, 2, 0.0, 65, 1920.0);
    let notes = sample_to_notes_grid(&occ, &grid, 1920.0, true);
    assert!(notes.contains(&(0.0, 65, 3840.0))); // 行0 连续
    assert!(notes.contains(&(0.0, 64, 1920.0))); // 行1 单格
    assert!(!notes.iter().any(|&(_, k, _)| k == 66));
}

#[test]
fn test_sample_to_notes_empty() {
    let occ: Vec<Vec<bool>> = vec![];
    let grid = horiz_grid(0, 0, 0.0, 60, 1920.0);
    assert!(sample_to_notes_grid(&occ, &grid, 1920.0, true).is_empty());
}

#[test]
fn test_rasterize_text_produces_ink() {
    // 仅在能加载到默认字体时验证（Windows 一般有 Microsoft YaHei；CI 缺失则跳过）
    if let Some(occ) = legacy_occ("A", 8, 8, "Microsoft YaHei") {
        assert!(occ.iter().flatten().any(|&b| b), "可识别字符应产生墨水");
    }
}

#[test]
fn test_rasterize_text_empty() {
    assert!(legacy_occ("", 8, 8, "Microsoft YaHei").is_none());
}

#[test]
fn test_rasterize_text_bottom_aligned() {
    // 文字应底部对齐到框底：下半区墨水应明显多于上半区，且顶部留白。
    // 本断言严格依赖 Microsoft YaHei 的字形度量（x-height/基线位置）。
    // `load_font` 缺失目标字体时会回退到首个可用字体（如 CI Linux 的 DejaVu），
    // 其 `c` 字形上下分布不同会导致 bottom>top 不成立（曾出现 50 vs 55 误报）。
    // 按本文件既有约定（“CI 缺失则跳过”）显式跳过，避免回退字体下的误报。
    let has_yahei = lumino_note_core::font_scanner::get_cached_fonts()
        .iter()
        .any(|f| {
            f.name == "Microsoft YaHei"
                || f.name.eq_ignore_ascii_case("Microsoft YaHei")
                || f.name.to_lowercase().contains("microsoft yahei")
        });
    if !has_yahei {
        eprintln!(
            "跳过 test_rasterize_text_bottom_aligned：缺失 Microsoft YaHei（回退字体不断言对齐）"
        );
        return;
    }
    if let Some(occ) = legacy_occ("c", 16, 16, "Microsoft YaHei") {
        let rows = occ.len();
        let half = rows / 2;
        let mut top_ink = 0u32;
        let mut bottom_ink = 0u32;
        for (r, row) in occ.iter().enumerate() {
            for &on in row.iter() {
                if on {
                    if r < half {
                        top_ink += 1;
                    } else {
                        bottom_ink += 1;
                    }
                }
            }
        }
        assert!(top_ink + bottom_ink > 0, "可识别字符应产生墨水");
        assert!(
            bottom_ink > top_ink,
            "文字应底部对齐：下半区墨水({bottom_ink})需多于上半区({top_ink})"
        );
    }
}

#[test]
fn test_text_tool_drag_x_snaps_to_precision() {
    // 拉框过程中 X 向长度必须按音符精度步进（与最终生成一致），不能自由像素。
    let mut editor = Editor::new();
    // 文字工具交互测试必须落在「非 Conductor 的可编辑轨」上（默认 track 0 为 Conductor）
    use crate::tests::test_helpers::seed_notes;
    seed_notes(&mut editor, 2, 1, &[]);
    // 设定视图使 pos_to_tick 为恒等映射，并设置音符精度
    editor.editor_state.view.zoom_x = 1.0;
    editor.editor_state.view.scroll_x = 0.0;
    editor.editor_state.view.keyboard_width = 0.0;
    let snap = 480.0;
    editor.editor_state.view.snap_precision = snap;

    // 按下新框：起点 tick=100 → 吸附到 0（round(100/480)=0）
    editor.handle_text_tool_pressed(Point::new(100.0, 100.0), 60);
    // 拖到 tick=700 → 吸附到 round(700/480)*480 = 480
    editor.handle_text_tool_moved(Point::new(700.0, 100.0));

    match &editor.editor_state.interaction.edit_state {
        EditState::Selecting {
            start_tick,
            current_tick,
            ..
        } => {
            assert!(
                (start_tick % snap).abs() < 1e-3,
                "起点 tick 应吸附精度: {start_tick}"
            );
            assert!(
                (current_tick % snap).abs() < 1e-3,
                "当前 tick 应吸附精度: {current_tick}"
            );
            let expected = (700.0 / snap).round() * snap;
            assert_eq!(*current_tick, expected, "X 向应吸附到精度网格");
        }
        other => panic!("拉框过程应处于 Selecting 状态，实际 {other:?}"),
    }
}

#[test]
fn test_text_tool_box_drag_move_wiring() {
    // 放置后的文本框：在中间实心区按下拖拽应整体移动，释放后清除拖拽态。
    use lumino_message::Tool;
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Text;
    // 预置已放置框需落在非 Conductor 轨（默认 track 0 为 Conductor，文字工具不可用）
    use crate::tests::test_helpers::seed_notes;
    seed_notes(&mut editor, 2, 1, &[]);
    let view = &mut editor.editor_state.view;
    view.zoom_x = 1.0;
    view.scroll_x = 0.0;
    view.keyboard_width = 0.0;
    view.zoom_y = 1.0;
    view.scroll_y = 0.0;
    view.ruler_height = 0.0;
    view.visible_key_count = 256;
    view.snap_precision = 480.0;

    // 预置已放置框：tick [480,960], key [60,64]
    let tt = &mut editor.editor_state.text_tool;
    tt.set_drag(480.0, 960.0, 60, 64);
    tt.active = true;
    tt.editing = true;

    // 框内按下（x∈[480,960], y∈[191,195]）：应进入拖拽移动
    editor.handle_text_tool_pressed(Point::new(600.0, 193.0), 62);
    assert!(
        editor.editor_state.text_tool.is_dragging(),
        "框内按下应进入拖拽移动"
    );

    // 拖到 x=1100（右移一个精度单元），y 不变
    editor.handle_text_tool_box_move(Point::new(1100.0, 193.0));
    let tt = &editor.editor_state.text_tool;
    assert_eq!(tt.start_tick, 960.0, "框应整体右移一个精度单元");
    assert_eq!(tt.end_tick, 1440.0, "宽度保持不变");
    assert_eq!(tt.start_key, 60);
    assert_eq!(tt.end_key, 64);

    // 释放 → 退出拖拽态
    editor.handle_released();
    assert!(
        !editor.editor_state.text_tool.is_dragging(),
        "释放后应清除拖拽临时状态"
    );
}

#[test]
fn test_text_tool_confirm_uses_incremental_path() {
    // 回归（Bug 2）：文字工具批量创建音符必须走「增量」路径（与铅笔/直线工具一致），
    // 绝不能用全量重建绕过。即确认后：note_delta_dirty 必须为 false（不触发
    // force_full_next），且 note_delta_events 携带正确的 InsertAt 增量事件，
    // 文档侧已写入音符。
    use crate::tests::test_helpers::seed_notes;
    use lumino_message::Tool;

    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Text;
    // 当前轨（非 Conductor 的普通轨 index=1）空文档：确认后的新音符从索引 0 起始
    seed_notes(&mut editor, 2, 1, &[]);

    // 构造一个能光栅化出音符的文字框（tick [0,480]，key [60,64]）
    let tt = &mut editor.editor_state.text_tool;
    tt.set_drag(0.0, 480.0, 60, 64);
    tt.active = true;
    tt.editing = true;
    tt.text = "A".to_string();
    tt.font_family = "Microsoft YaHei";

    // 字体环境探测：macOS/headless CI 可能无 Microsoft YaHei，且回退字体
    // 也可能不可读（font-kit 句柄/沙箱限制），此时光栅化必然失败。
    // 本测试验证的是「增量路径」而非字体渲染，按本文件既有约定
    //（“CI 缺失则跳过”）显式跳过，避免环境型误报。
    if rasterize_text(
        "A",
        8,
        8,
        "Microsoft YaHei",
        &RasterScale::legacy(false),
        lumino_core::TextAlign::Left,
    )
    .is_none()
    {
        eprintln!(
            "跳过 test_text_tool_confirm_uses_incremental_path：无可用系统字体（光栅化探测失败）"
        );
        return;
    }

    assert!(
        editor.confirm_text_tool(),
        "确认应成功光栅化并创建音符（依赖系统字体回退）"
    );

    // 必须走增量：不触发全量重建（note_delta_dirty 保持 false）
    assert!(
        !editor.editor_state.data.note_delta_dirty,
        "文字工具确认必须走增量路径，不得触发全量重建（force_full_next）"
    );

    // 增量事件已记录：携带 InsertAt（供渲染线程段内插入）
    let inserts: Vec<_> = editor
        .editor_state
        .data
        .note_delta_events
        .iter()
        .filter(|e| matches!(e, lumino_editor_state::NoteDeltaEvent::InsertAt { .. }))
        .collect();
    assert!(!inserts.is_empty(), "确认后应产生 InsertAt 增量事件");

    // 文档侧：音符确实已写入当前轨（普通轨 index=1，非 Conductor）
    let created = editor.editor_state.data.track_notes(1).len();
    assert!(created > 0, "确认后当前轨应至少写入一个音符");
}

#[test]
fn test_confirm_text_creates_persistent_text_shape() {
    let mut editor = text_tool_editor(false);
    {
        let tt = &mut editor.editor_state.text_tool;
        tt.set_drag(0.0, 960.0, 60, 64);
        tt.active = true;
        tt.editing = true;
        tt.text = "A".to_string();
        tt.font_family = "Microsoft YaHei";
        tt.params.threshold = 40.0;
    }
    if rasterize_text(
        "A",
        8,
        8,
        "Microsoft YaHei",
        &RasterScale::legacy(false),
        lumino_core::TextAlign::Left,
    )
    .is_none()
    {
        eprintln!("跳过 test_confirm_text_creates_persistent_text_shape：无可用系统字体");
        return;
    }
    assert!(editor.confirm_text_tool());

    let shapes = editor.editor_state.shape_store.persisted_models();
    assert_eq!(shapes.len(), 1, "确认文字应持久化一个文字形状");
    let model = &shapes[0];
    assert!(model.geometry.rect.is_some(), "文字形状必须有文本框 bbox");
    let text_op = model
        .ops
        .iter()
        .find_map(|op| match op {
            lumino_core::shape::ShapeOp::TextV2(op) => Some(op),
            _ => None,
        })
        .expect("文字形状必须带 TextOp");
    assert_eq!(text_op.text, "A");
    assert_eq!(text_op.params.threshold, 40.0);
    assert_eq!(text_op.snap, editor.editor_state.view.snap_precision);
    assert!(!text_op.merged);
    assert!(
        text_op.k.is_finite() && text_op.k > 0.0,
        "必须冻结制字时的 beats per key，实际 {}",
        text_op.k
    );
    assert_eq!(model.k, text_op.k, "形状 k 与算子 k 必须一致");
}

#[test]
fn test_click_existing_text_shape_is_read_only() {
    let mut editor = text_tool_editor(false);
    {
        let tt = &mut editor.editor_state.text_tool;
        tt.set_drag(0.0, 960.0, 60, 64);
        tt.active = true;
        tt.editing = true;
        tt.text = "A".to_string();
        tt.font_family = "Microsoft YaHei";
    }
    if rasterize_text(
        "A",
        8,
        8,
        "Microsoft YaHei",
        &RasterScale::legacy(false),
        lumino_core::TextAlign::Left,
    )
    .is_none()
    {
        eprintln!("跳过 test_click_existing_text_shape_is_read_only：无可用系统字体");
        return;
    }
    assert!(editor.confirm_text_tool());
    let shape_id = editor.editor_state.shape_store.ids()[0];
    let before_rect = editor
        .editor_state
        .shape_store
        .model(shape_id)
        .and_then(|m| m.geometry.rect);
    let text_before = editor
        .editor_state
        .shape_store
        .model(shape_id)
        .and_then(|m| {
            m.ops.iter().find_map(|op| match op {
                lumino_core::shape::ShapeOp::TextV2(op) => Some(op.text.clone()),
                _ => None,
            })
        })
        .expect("文字形状应保留 TextOp");
    let notes_before: Vec<_> = editor
        .editor_state
        .data
        .current_track_notes()
        .iter()
        .copied()
        .collect();

    let pos = editor.tick_key_to_pos(480.0, 62);
    editor.handle_text_tool_pressed(pos, 62);

    // DEC-001：已 Bake 的文字形状只读，点击只消费事件，不进入重新编辑。
    assert!(
        !editor.editor_state.text_tool.active,
        "不得在已 Bake 文字上重新打开文本框"
    );
    assert_eq!(
        editor.editor_state.shape_store.ids(),
        vec![shape_id],
        "不得新建第二个文字形状"
    );
    let text_after = editor
        .editor_state
        .shape_store
        .model(shape_id)
        .and_then(|m| {
            m.ops.iter().find_map(|op| match op {
                lumino_core::shape::ShapeOp::TextV2(op) => Some(op.text.clone()),
                _ => None,
            })
        })
        .expect("文字形状应保留 TextOp");
    assert_eq!(text_after, text_before, "只读点击不得改文字");
    assert_eq!(
        editor
            .editor_state
            .shape_store
            .model(shape_id)
            .and_then(|m| m.geometry.rect),
        before_rect,
        "只读点击不得改文本框位置"
    );
    let notes_after: Vec<_> = editor
        .editor_state
        .data
        .current_track_notes()
        .iter()
        .copied()
        .collect();
    assert_eq!(notes_after, notes_before, "只读点击不得改动音符");
}

#[test]
fn test_text_shape_participates_in_undo_redo() {
    let mut editor = text_tool_editor(false);
    {
        let tt = &mut editor.editor_state.text_tool;
        tt.set_drag(0.0, 960.0, 60, 64);
        tt.active = true;
        tt.editing = true;
        tt.text = "A".to_string();
        tt.font_family = "Microsoft YaHei";
    }
    if rasterize_text(
        "A",
        8,
        8,
        "Microsoft YaHei",
        &RasterScale::legacy(false),
        lumino_core::TextAlign::Left,
    )
    .is_none()
    {
        eprintln!("跳过 test_text_shape_participates_in_undo_redo：无可用系统字体");
        return;
    }
    assert!(editor.confirm_text_tool());
    assert_eq!(editor.editor_state.shape_store.persisted_models().len(), 1);
    assert!(editor.undo());
    assert!(
        editor
            .editor_state
            .shape_store
            .persisted_models()
            .is_empty(),
        "undo 应隐藏本次创建的文字形状"
    );
    assert!(editor.redo());
    assert_eq!(
        editor.editor_state.shape_store.persisted_models().len(),
        1,
        "redo 应恢复文字形状"
    );
}

#[test]
fn test_text_tool_confirm_rejected_on_conductor_track() {
    // 回归：文字工具不得在非可编辑的 Conductor 音轨（track 0）放置音符，
    // 与铅笔等工具（finish_drawing 的 `current_track == 0` 守卫）保持一致。
    use crate::tests::test_helpers::seed_notes;
    use lumino_message::Tool;

    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Text;
    // 选中 Conductor 音轨（track 0），且无任何音符
    seed_notes(&mut editor, 1, 0, &[]);

    let tt = &mut editor.editor_state.text_tool;
    tt.set_drag(0.0, 480.0, 60, 64);
    tt.active = true;
    tt.editing = true;
    tt.text = "A".to_string();
    tt.font_family = "Microsoft YaHei";

    // Conductor 音轨禁止放置：确认必须失败，且文档侧不得写入任何音符
    assert!(
        !editor.confirm_text_tool(),
        "Conductor 音轨（track 0）禁止放置音符，确认必须返回 false"
    );
    let created = editor.editor_state.data.track_notes(0).len();
    assert_eq!(created, 0, "Conductor 音轨不应写入任何音符");
}

mod part2;
