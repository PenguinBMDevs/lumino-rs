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
