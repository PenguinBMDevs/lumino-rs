//! 工程走带「框选误伤」回归：批量框选拖动 / 批量变速后，选择集必须永久固定为
//! **框选时内部包含的被框选音符**，而不是「矩形平移到落点后覆盖到的音符」。
//!
//! 缺陷背景：走带选择以矩形描述，几何变更（拖动平移 / 变速缩放）后矩形不再
//! 与音符一致——拖动提交后由调用方把矩形整体平移到落点，落点区域内**本来不在
//! 框选内**的既有音符随之进入选择；变速则完全不更新矩形。两者都会让后续操作
//! （再次拖动 / 删除 / 复制 / 变速 / ghost 预览）误伤非框选音符。
//! 修复：几何变更提交后把选择集冻结为本次实际变更的音符（精确
//! `(视觉音轨, start_tick, key)` 集合），矩形降级为紧致边界。

use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers::seed_notes;

/// 音轨音符值列表 `(start_tick, key)`（升序，按值断言避免索引漂移干扰）
fn track_values(editor: &Editor, track: usize) -> Vec<(u32, u8)> {
    editor
        .editor_state
        .data
        .track_notes(track)
        .iter()
        .map(|n| (n.start_tick, n.key))
        .collect()
}

/// 走带选择当前命中的音符值列表 `(start_tick, key)`（跨音轨，按值排序）
fn selected_values(editor: &Editor) -> Vec<(u32, u8)> {
    let mut values: Vec<(u32, u8)> = editor
        .arrangement_selected_notes()
        .into_iter()
        .map(|(start, _, _, key)| (start as u32, key))
        .collect();
    values.sort_unstable();
    values
}

/// 单轨场景：框选 [50, 450) 的两个音符拖到落点区域，落点既有音符不得被误伤
#[test]
fn test_arrangement_move_freezes_selection_without_friendly_fire() {
    let mut editor = Editor::default();
    seed_notes(
        &mut editor,
        1,
        0,
        &[
            Note::from_raw(100.0, 60, 100.0, 100, 0),  // 框选对象 A
            Note::from_raw(300.0, 60, 100.0, 100, 0),  // 框选对象 B
            Note::from_raw(1150.0, 64, 100.0, 100, 0), // 落点既有音符 V1
            Note::from_raw(1350.0, 64, 100.0, 100, 0), // 落点既有音符 V2
        ],
    );

    // 框选：矩形覆盖 A、B（不含落点既有音符）
    editor
        .editor_state
        .data
        .arrange_selection
        .add_rect_track(50, 450, 0, 127, 0, 0);
    assert_eq!(
        selected_values(&editor),
        vec![(100, 60), (300, 60)],
        "框选应命中 A、B"
    );

    // 拖动 +1000 tick（A/B 落到 1100/1300，与落点既有音符 V1/V2 区域重叠）
    let moved = editor.arrange_move_notes(1000, 0);
    assert_eq!(moved, 2, "应移动 A、B 两个音符");

    // 选择集必须**永久固定**为移动后的 A、B，而非矩形覆盖区内的 V1/V2
    assert_eq!(
        selected_values(&editor),
        vec![(1100, 60), (1300, 60)],
        "拖动后选择集应恰为被移动的音符（框选误伤修复核心）"
    );
    let selection = &editor.editor_state.data.arrange_selection;
    assert!(
        !selection.contains(0, 1150, 64) && !selection.contains(0, 1350, 64),
        "落点既有音符不得进入选择集"
    );
    // 矩形降级为紧致边界（显示 / 命中用）
    assert_eq!(
        selection.rects,
        vec![(1100, 1400, 60, 60, 0, 0)],
        "选择矩形应为移动后音符的紧致边界"
    );

    // 第二次拖动：只应继续搬运 A、B，落点既有音符必须原地不动
    let moved_again = editor.arrange_move_notes(200, 0);
    assert_eq!(moved_again, 2, "第二次拖动仍只作用于原框选音符");
    assert_eq!(
        track_values(&editor, 0),
        vec![(1150, 64), (1300, 60), (1350, 64), (1500, 60)],
        "V1/V2 应保持原位，仅 A/B 继续偏移"
    );
    assert_eq!(
        selected_values(&editor),
        vec![(1300, 60), (1500, 60)],
        "第二次拖动后选择集仍为 A/B"
    );
}

/// 跨音轨场景：框选覆盖两轨，落点区域内其他轨的既有音符同样不得被误伤
#[test]
fn test_arrangement_move_freeze_across_tracks() {
    let mut editor = Editor::default();
    seed_notes(
        &mut editor,
        2,
        0,
        &[
            Note::from_raw(100.0, 60, 100.0, 100, 0),  // 轨 0 框选对象 A
            Note::from_raw(1150.0, 64, 100.0, 100, 0), // 轨 0 落点既有音符 V1
        ],
    );
    editor
        .editor_state
        .data
        .insert_note(1, Note::from_raw(300.0, 62, 100.0, 100, 0)); // 轨 1 框选对象 B
    editor
        .editor_state
        .data
        .insert_note(1, Note::from_raw(1350.0, 66, 100.0, 100, 0)); // 轨 1 落点既有音符 V2

    // 框选：覆盖视觉轨 0..=1
    editor
        .editor_state
        .data
        .arrange_selection
        .add_rect_track(50, 450, 0, 127, 0, 1);
    assert_eq!(selected_values(&editor), vec![(100, 60), (300, 62)]);

    let moved = editor.arrange_move_notes(1000, 0);
    assert_eq!(moved, 2);
    assert_eq!(
        selected_values(&editor),
        vec![(1100, 60), (1300, 62)],
        "跨轨拖动后选择集应恰为被移动的音符"
    );

    let selection = &editor.editor_state.data.arrange_selection;
    assert!(
        !selection.contains(0, 1150, 64) && !selection.contains(1, 1350, 66),
        "两轨落点既有音符均不得进入选择集"
    );

    // 再次拖动：V1/V2 原地不动
    assert_eq!(editor.arrange_move_notes(200, 0), 2);
    assert_eq!(track_values(&editor, 0), vec![(1150, 64), (1300, 60)]);
    assert_eq!(track_values(&editor, 1), vec![(1350, 66), (1500, 62)]);
}

/// 空命中：拖动手势落在无音符区域时不改动选择集（不产生空冻结）
#[test]
fn test_arrangement_move_without_hits_keeps_selection() {
    let mut editor = Editor::default();
    seed_notes(
        &mut editor,
        1,
        0,
        &[Note::from_raw(100.0, 60, 100.0, 100, 0)],
    );
    editor
        .editor_state
        .data
        .arrange_selection
        .add_rect_track(2000, 3000, 0, 127, 0, 0);

    let moved = editor.arrange_move_notes(100, 0);
    assert_eq!(moved, 0, "无命中音符时不移动");
    let selection = &editor.editor_state.data.arrange_selection;
    assert!(selection.frozen().is_none(), "不应产生冻结集");
    assert_eq!(selection.rects, vec![(2000, 3000, 0, 127, 0, 0)]);
}

/// 视觉音轨映射：冻结条目必须记录**视觉位置**（侧边栏顺序），与 `contains` 同空间
///
/// 音轨排序后视觉位置 ≠ 文档索引；若冻结时误用文档索引，跨轨拖动后的选择会
/// 落到错误音轨（框选误伤的另一种形态）。
#[test]
fn test_freeze_uses_visual_track_position() {
    let mut editor = Editor::default();
    seed_notes(
        &mut editor,
        3,
        0,
        &[Note::from_raw(100.0, 60, 100.0, 100, 0)], // doc 轨 0
    );
    // 排序后的视觉映射：视觉 0 = doc 2，视觉 1 = doc 0，视觉 2 = doc 1
    editor.editor_state.data.track_visual_order = vec![2, 0, 1];
    assert_eq!(editor.editor_state.data.visual_position_of(0), Some(1));
    assert_eq!(editor.editor_state.data.visual_position_of(1), Some(2));

    // 框选视觉轨 1（= doc 轨 0）上的音符
    editor
        .editor_state
        .data
        .arrange_selection
        .add_rect_track(50, 450, 0, 127, 1, 1);
    assert_eq!(selected_values(&editor), vec![(100, 60)]);

    // 跨轨下移 1 行：视觉 1 → 视觉 2（= doc 轨 1）
    assert_eq!(editor.arrange_move_notes(0, 1), 1);
    assert_eq!(
        track_values(&editor, 1),
        vec![(100, 60)],
        "音符应落到 doc 轨 1（视觉 2）"
    );

    let selection = &editor.editor_state.data.arrange_selection;
    assert!(
        selection.contains(2, 100, 60),
        "冻结条目应按视觉位置（2）记录，而非文档索引（1）"
    );
    assert!(!selection.contains(1, 100, 60));
    assert_eq!(
        selection.rects,
        vec![(100, 200, 60, 60, 2, 2)],
        "紧致边界同样按视觉位置给出"
    );
}

/// 变速同类修复：变速后选择集收敛为被变速音符，紧致边界内的非框选音符不得被误伤
#[test]
fn test_arrange_speed_change_freezes_selection() {
    let mut editor = Editor::default();
    seed_notes(
        &mut editor,
        1,
        0,
        &[
            Note::from_raw(0.0, 60, 240.0, 100, 0),   // 框选对象 A
            Note::from_raw(120.0, 62, 40.0, 100, 0),  // 非框选音符 V（落在变速后紧致边界内）
            Note::from_raw(960.0, 64, 240.0, 100, 0), // 框选对象 B
        ],
    );
    editor
        .editor_state
        .data
        .arrange_selection
        .add_rect_track(0, 100, 0, 127, 0, 0);
    editor
        .editor_state
        .data
        .arrange_selection
        .add_rect_track(900, 1000, 0, 127, 0, 0);
    assert_eq!(selected_values(&editor), vec![(0, 60), (960, 64)]);

    // factor 0.25：A → 0..60，B → 240..300（V 未选中、不动）
    assert_eq!(editor.arrange_apply_speed_change(0.25), 2);

    assert_eq!(
        selected_values(&editor),
        vec![(0, 60), (240, 64)],
        "变速后选择集应恰为被变速音符（新位置）"
    );
    let selection = &editor.editor_state.data.arrange_selection;
    assert!(
        !selection.contains(0, 120, 62),
        "紧致边界内的非框选音符不得进入选择集"
    );
    assert_eq!(
        selection.rects,
        vec![(0, 300, 60, 64, 0, 0)],
        "选择矩形应为变速后音符的紧致边界"
    );

    // 后续拖动只搬运被变速的两个音符，V 必须原地不动
    assert_eq!(editor.arrange_move_notes(1000, 0), 2);
    assert_eq!(
        track_values(&editor, 0),
        vec![(120, 62), (1000, 60), (1240, 64)],
        "V 应保持原位"
    );
}

/// 新框选取代冻结集：拖动后重新框选必须回到矩形语义
#[test]
fn test_new_box_selection_drops_frozen_set() {
    let mut editor = Editor::default();
    seed_notes(
        &mut editor,
        1,
        0,
        &[
            Note::from_raw(100.0, 60, 100.0, 100, 0),
            Note::from_raw(1150.0, 64, 100.0, 100, 0),
        ],
    );
    editor
        .editor_state
        .data
        .arrange_selection
        .add_rect_track(50, 450, 0, 127, 0, 0);
    assert_eq!(editor.arrange_move_notes(1000, 0), 1);
    assert!(
        !editor
            .editor_state
            .data
            .arrange_selection
            .contains(0, 1150, 64),
        "拖动后落点既有音符未被误伤"
    );

    // 用户重新框选（先清空 + 添加矩形，与 ArrangementSelectionChanged 一致）
    editor.editor_state.data.arrange_selection.clear();
    editor
        .editor_state
        .data
        .arrange_selection
        .add_rect_track(1100, 1300, 0, 127, 0, 0);
    assert_eq!(
        selected_values(&editor),
        vec![(1100, 60), (1150, 64)],
        "新框选按矩形覆盖区域内全部音符（含 1150 落点既有音符）"
    );
}
