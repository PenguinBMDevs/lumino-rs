use super::*;
use crate::{EditState, Editor};
use iced_core::Point;

#[test]
fn test_sample_to_notes_normal_single_cell() {
    let occ = vec![vec![true]];
    let notes = sample_to_notes(&occ, 0.0, 64, 1920.0, false);
    assert_eq!(notes, vec![(0.0, 64, 1920.0)]);
}

#[test]
fn test_sample_to_notes_normal_grid() {
    // 2x2 全占用：正常模式生成 4 个独立音符，长度均为 snap
    let occ = vec![vec![true, true], vec![true, true]];
    let notes = sample_to_notes(&occ, 100.0, 70, 480.0, false);
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
    let notes = sample_to_notes(&occ, 0.0, 60, 1920.0, true);
    assert_eq!(notes, vec![(0.0, 60, 1920.0), (3840.0, 60, 3840.0)]);
}

#[test]
fn test_sample_to_notes_merged_gap_breaks() {
    // 相邻但有空隙的行：空隙处必须断开（不合并本应分开的笔画）
    let occ = vec![vec![true, false, true]];
    let notes = sample_to_notes(&occ, 0.0, 60, 100.0, true);
    // [0..0] len 100, [2..2] len 100
    assert_eq!(notes, vec![(0.0, 60, 100.0), (200.0, 60, 100.0)]);
}

#[test]
fn test_sample_to_notes_merged_multiline_keys() {
    // 两行：key_top=65 → 行 0 = 65，行 1 = 64
    let occ = vec![vec![true, true], vec![true, false]];
    let notes = sample_to_notes(&occ, 0.0, 65, 1920.0, true);
    assert!(notes.contains(&(0.0, 65, 3840.0))); // 行0 连续
    assert!(notes.contains(&(0.0, 64, 1920.0))); // 行1 单格
    assert!(!notes.iter().any(|&(_, k, _)| k == 66));
}

#[test]
fn test_sample_to_notes_empty() {
    let occ: Vec<Vec<bool>> = vec![];
    assert!(sample_to_notes(&occ, 0.0, 60, 1920.0, true).is_empty());
}

#[test]
fn test_rasterize_text_produces_ink() {
    // 仅在能加载到默认字体时验证（Windows 一般有 Microsoft YaHei；CI 缺失则跳过）
    if let Some(occ) = rasterize_text("A", 8, 8, "Microsoft YaHei") {
        assert!(occ.iter().flatten().any(|&b| b), "可识别字符应产生墨水");
    }
}

#[test]
fn test_rasterize_text_empty() {
    assert!(rasterize_text("", 8, 8, "Microsoft YaHei").is_none());
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
    if let Some(occ) = rasterize_text("c", 16, 16, "Microsoft YaHei") {
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
    if rasterize_text("A", 8, 8, "Microsoft YaHei").is_none() {
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

// ───────────────────────── 纵向卷帘（转置）回归 ─────────────────────────

/// 构造纵向卷帘下的文字工具编辑器（画布 800×600、底部键盘 120 → 网格 y ∈ [0,480)）。
///
/// 视图固定：`zoom_x = 1`（tick↔Y 恒等比例）、`zoom_y = 4`（key↔X 每键 4px）、
/// 无滚动、`snap = 480`。落点换算：`key = x/4`、`tick = 480 - y`。
fn vertical_text_editor() -> Editor {
    use crate::tests::test_helpers::seed_notes;
    use lumino_message::Tool;

    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Text;
    // 文字工具在 Conductor（track 0）整工具不可用：交互测试必须落在普通轨
    seed_notes(&mut editor, 2, 1, &[]);
    editor.editor_state.is_vertical_roll = true;
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

/// 纵向卷帘端到端回归（BUG：文字工具在纵向"能拉框、点了没反应"）。
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
    if rasterize_text("A", cols, rows, family).is_none() {
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

/// 「所见即生成」的**映射层**证明（纵向）：转置后的预览墨格与 `sample_to_notes`
/// 生成的音符格一一对应。
///
/// 生成侧口径（`rasterize.rs`）：行→key（行 0 = 最高 key）、列→tick（列 0 = 起始 tick）。
/// 纵向视图口径：key→X（越大越右）、tick→Y（越大越上）。两者合成后，预览位图在屏幕上
/// 相对光栅化位图是一个**镜像变换**——若预览直接铺原图，就成了"预览正立、生成镜像"。
/// 本测试把两侧换算到同一屏幕格上做集合相等，防止任何一侧被单独改动。
#[test]
fn test_vertical_preview_cells_match_generated_notes() {
    use crate::grid::text_tool_box::transpose_preview_rgba;

    let snap = 480.0;
    let (tick_lo, key_top) = (960.0, 64i32);
    // 合成占用网格（2 行 × 2 列）：行 0 = key 64、行 1 = key 63；列 0 = tick 960、列 1 = 1440
    let occ = vec![vec![true, false], vec![false, true]];
    let notes = sample_to_notes(&occ, tick_lo, key_top, snap, false);
    assert_eq!(
        notes,
        vec![(tick_lo, 64, snap), (tick_lo + snap, 63, snap)],
        "生成侧口径：行→key、列→tick"
    );

    // 与光栅化同布局的位图（源 x = col、源 y = row，1 像素 / 格）
    let (w, h) = (2u32, 2u32);
    let mut src = vec![0u8; (w * h * 4) as usize];
    for (row, row_ink) in occ.iter().enumerate() {
        for (col, &on) in row_ink.iter().enumerate() {
            if on {
                src[((row as u32 * w + col as u32) * 4) as usize + 3] = 255;
            }
        }
    }
    let dst = transpose_preview_rgba(&src, w, h);
    let mut preview_cells: Vec<(usize, usize)> = Vec::new();
    for y in 0..h as usize {
        for x in 0..w as usize {
            if dst[(y * w as usize + x) * 4 + 3] > 0 {
                preview_cells.push((x, y));
            }
        }
    }
    preview_cells.sort_unstable();

    // 由音符反推期望屏幕格：x = (列数-1) − row（key 越大越右）、y = (行数-1) − col（tick 越大越上）
    let mut note_cells: Vec<(usize, usize)> = notes
        .iter()
        .map(|&(tick, key, _)| {
            let col = ((tick - tick_lo) / snap).round() as usize;
            let row = (key_top - key as i32) as usize;
            (h as usize - 1 - row, w as usize - 1 - col)
        })
        .collect();
    note_cells.sort_unstable();

    assert_eq!(
        preview_cells, note_cells,
        "纵向预览墨格必须与 √ 生成的音符格逐格一致（否则就是「看到一套、生成另一套」）"
    );
}
