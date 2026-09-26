//! 走带剪贴板单元测试（原 clipboard.rs 测试模块）

use crate::Editor;
use crate::clipboard::ARRANGEMENT_BINARY_MARK;
use crate::note::Note;
use crate::tests::test_helpers::{doc_with_notes, seed_notes};
use lumino_midi_loader::NoteEvent;

fn editor_with_sorted_visual_order() -> Editor {
    let mut editor = Editor::default();
    let notes2 = vec![Note::from_raw(0.0, 64, 10.0, 100, 0)];
    editor.editor_state.data.document = Some(doc_with_notes(3, 2, &notes2));
    editor
        .editor_state
        .data
        .insert_note(0, Note::from_raw(0.0, 60, 10.0, 100, 0));
    editor.editor_state.data.track_visual_order = vec![2, 0, 1];
    editor
}

fn doc_track_note_count(editor: &Editor, track: usize) -> usize {
    editor.editor_state.data.track_notes(track).len()
}

#[test]
fn test_compute_anchor_visual_maps_to_document_track() {
    let mut editor = editor_with_sorted_visual_order();
    editor.editor_state.data.current_track = 0;
    assert_eq!(editor.compute_anchor_visual(), 1);
    // 经 `add_rect_track` 写入（bump revision）——直接改 `rects` 会绕过版本号，
    // 让依赖 revision 的派生缓存读到脏数据
    editor
        .editor_state
        .data
        .arrange_selection
        .add_rect_track(0, 10, 0, 127, 0, 0);
    assert_eq!(editor.compute_anchor_visual(), 0);
    editor.editor_state.data.arrange_selection.clear();
    editor
        .editor_state
        .data
        .arrange_selection
        .add_rect_track(0, 10, 0, 127, 1, 1);
    assert_eq!(editor.compute_anchor_visual(), 1);
}

fn single_track_clipboard_json() -> String {
    r#"{"type":"arrangement","origin_tick":0.0,"origin_key":64,"origin_track":0,"division":480,"notes":[{"tick":0.0,"key":0,"length":10.0,"velocity":100,"channel":0,"track":0}]}"#.to_string()
}

#[test]
fn test_paste_lands_on_mapped_document_track() {
    let mut editor = editor_with_sorted_visual_order();
    editor
        .editor_state
        .data
        .arrange_selection
        .rects
        .push((0, 10, 0, 127, 0, 0));
    let pasted = editor.arrange_paste_from_text(&single_track_clipboard_json());
    assert!(pasted, "粘贴应成功");
    assert_eq!(
        doc_track_note_count(&editor, 2),
        2,
        "doc 轨 2 应新增 1 个音符"
    );
    assert_eq!(doc_track_note_count(&editor, 0), 1, "doc 轨 0 不应被误写入");
    assert_eq!(doc_track_note_count(&editor, 1), 0);
}

#[test]
fn test_paste_multi_track_preserves_visual_layout() {
    let mut editor = editor_with_sorted_visual_order();
    editor
        .editor_state
        .data
        .arrange_selection
        .rects
        .push((0, 10, 0, 127, 0, 1));
    let json = r#"{"type":"arrangement","origin_tick":0.0,"origin_key":60,"origin_track":0,"division":480,"notes":[{"tick":0.0,"key":0,"length":10.0,"velocity":100,"channel":0,"track":0},{"tick":0.0,"key":0,"length":10.0,"velocity":100,"channel":0,"track":1}]}"#;
    let pasted = editor.arrange_paste_from_text(json);
    assert!(pasted, "多轨粘贴应成功");
    assert_eq!(doc_track_note_count(&editor, 2), 2, "doc 轨 2 应新增音符");
    assert_eq!(doc_track_note_count(&editor, 0), 2, "doc 轨 0 应新增音符");
    assert_eq!(doc_track_note_count(&editor, 1), 0, "doc 轨 1 不应被误写入");
}

#[test]
fn test_paste_identity_mapping_unchanged() {
    let mut editor = Editor::default();
    seed_notes(&mut editor, 3, 0, &[Note::from_raw(0.0, 60, 10.0, 100, 0)]);
    editor
        .editor_state
        .data
        .arrange_selection
        .rects
        .push((0, 10, 0, 127, 1, 1));
    let json = r#"{"type":"arrangement","origin_tick":0.0,"origin_key":60,"origin_track":0,"division":480,"notes":[{"tick":0.0,"key":0,"length":10.0,"velocity":100,"channel":0,"track":0}]}"#;
    let pasted = editor.arrange_paste_from_text(json);
    assert!(pasted);
    assert_eq!(doc_track_note_count(&editor, 1), 1);
    assert_eq!(doc_track_note_count(&editor, 0), 1);
}

#[test]
fn test_track_switch_clears_arrange_selection_so_paste_anchors_switched_track() {
    let mut editor = editor_with_sorted_visual_order();
    editor.editor_state.data.current_track = 2;
    editor
        .editor_state
        .data
        .arrange_selection
        .rects
        .push((0, 100, 0, 127, 0, 0));
    editor.switch_to_track(0);
    let json = single_track_clipboard_json();
    let pasted = editor.arrange_paste_from_text(&json);
    assert!(pasted, "粘贴应成功");
    assert_eq!(
        doc_track_note_count(&editor, 0),
        2,
        "粘贴应落到切换后的当前轨 doc 0（而非旧选区所在 doc 2）"
    );
    assert_eq!(
        doc_track_note_count(&editor, 2),
        1,
        "doc 2 不应被旧选区误写入"
    );
}

/// 回归：PPQN 不一致时粘贴需多一次重采样，使音符**长度（节拍）完全一致**。
/// 源 division=480、length=480（=1 拍）；目标 division=960 → 应重采样为 960。
#[test]
fn test_paste_resamples_length_on_ppqn_mismatch() {
    let mut editor = Editor::default();
    editor.editor_state.data.document = Some(doc_with_notes(1, 0, &[]));
    // 目标文档 PPQN 设为 960（与源 480 不一致）
    editor
        .editor_state
        .data
        .document
        .as_mut()
        .expect("测试前已设置 document，应能取得可变借用")
        .division = 960;
    let json = r#"{"type":"arrangement","origin_tick":0.0,"origin_key":60,"origin_track":0,"division":480,"notes":[{"tick":0.0,"key":0,"length":480.0,"velocity":100,"channel":0,"track":0}]}"#;
    assert!(editor.arrange_paste_from_text(json));
    let notes = editor.editor_state.data.track_notes(0);
    assert_eq!(notes.len(), 1);
    // 1 拍在 480 PPQN 下 = 480 tick；在 960 PPQN 下应 = 960 tick（长度一致）
    assert_eq!(
        notes[0].length(),
        960u32,
        "PPQN 不一致时应重采样长度以保节拍一致"
    );
}

/// 回归：PPQN 一致（或缺失 division）时零缩放，粘贴音符长度逐字节一致。
#[test]
fn test_paste_no_resample_when_ppqn_matches() {
    let mut editor = Editor::default();
    editor.editor_state.data.document = Some(doc_with_notes(1, 0, &[]));
    // 默认 doc_with_notes division=480，与 JSON division=480 一致
    let json = r#"{"type":"arrangement","origin_tick":0.0,"origin_key":60,"origin_track":0,"division":480,"notes":[{"tick":0.0,"key":0,"length":480.0,"velocity":100,"channel":0,"track":0}]}"#;
    assert!(editor.arrange_paste_from_text(json));
    let notes = editor.editor_state.data.track_notes(0);
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].length(), 480u32, "PPQN 一致时不应缩放");
}

/// 回归：二进制走带剪贴板往返与 JSON 路径**落轨一致**，且头哨兵正确。
///
/// 与 `test_paste_identity_mapping_unchanged` 同构：粘贴目标带 selection 视觉区间 (1,1)
/// （anchor_visual=1）、二进制音符 track 偏移=0，粘贴应落到 doc 1（而非 doc 0），
/// 与 JSON 路径逐字节一致。
#[test]
fn test_arrangement_binary_roundtrip_matches_json() {
    let mut editor = Editor::default();
    seed_notes(&mut editor, 3, 0, &[Note::from_raw(0.0, 60, 10.0, 100, 0)]);
    // 单音符（doc 0，视觉 0 → track 偏移 0），division=480，origin_key=60，origin_tick=0
    let all_notes = vec![(0usize, NoteEvent::new(0, 10, 60, 100, 0))];
    let bytes = editor
        .encode_arrangement_clipboard_binary(&all_notes)
        .expect("二进制编码失败");
    let meta = lumino_midi_model::clipboard::parse_clipboard_header(&bytes).expect("头解析失败");
    assert_eq!(
        meta.track_hint, ARRANGEMENT_BINARY_MARK,
        "二进制头应写入走带子格式哨兵"
    );

    // 粘贴目标带 selection 视觉区间 (1,1) → anchor_visual=1，与 JSON 测试一致
    editor
        .editor_state
        .data
        .arrange_selection
        .rects
        .push((0, 10, 0, 127, 1, 1));
    assert!(
        editor.arrange_paste_from_binary_bytes(&bytes),
        "二进制粘贴应成功"
    );
    assert_eq!(
        doc_track_note_count(&editor, 1),
        1,
        "二进制粘贴落轨应与 JSON 路径一致（doc 1）"
    );
    assert_eq!(
        doc_track_note_count(&editor, 0),
        1,
        "原始种子音符仍应在 doc 0"
    );
}

// ══════════════════════════════════════════════════════════════════
// 跨视图互通（卷帘 ⇄ 走带）
//
// 缺陷背景：两套剪贴板子格式互斥——走带粘贴硬拒非哨兵二进制、硬拒非
// `type=arrangement` 的 JSON，卷帘粘贴则把 per-note track 字段直接丢弃。
// 结果「卷帘复制 → 走带粘贴」静默失败，「走带复制 → 卷帘粘贴」把所有音轨的
// 音符叠进当前轨（看起来成功、结果全错）。
//
// 修复：统一 `ClipRecord.track` 语义为「相对锚点的视觉偏移」（卷侧恒 0），
// 走带解除哨兵硬拒，卷帘识别走带子格式后按偏移多轨路由。
// ══════════════════════════════════════════════════════════════════

/// 卷帘复制 → 走带粘贴（二进制）：卷侧 track 偏移恒 0，应落到走带锚点轨
#[test]
fn test_piano_roll_binary_pastes_into_arrangement() {
    let mut editor = Editor::default();
    seed_notes(&mut editor, 3, 0, &[Note::from_raw(0.0, 60, 10.0, 100, 0)]);
    editor.selection_insert(0);

    let bytes = editor
        .build_clipboard_binary(0, 480)
        .expect("卷帘二进制编码失败");
    let meta = lumino_midi_model::clipboard::parse_clipboard_header(&bytes).expect("头解析失败");
    assert_ne!(
        meta.track_hint, ARRANGEMENT_BINARY_MARK,
        "卷帘子格式不应写走带哨兵"
    );

    // 走带锚点：矩形视觉轨 (1,1) → anchor_visual=1 → doc 1（identity 视觉序）
    editor
        .editor_state
        .data
        .arrange_selection
        .add_rect_track(0, 10, 0, 127, 1, 1);
    assert!(
        editor.arrange_paste_from_binary_bytes(&bytes),
        "卷帘载荷必须能被走带粘贴（原缺陷：静默失败）"
    );
    assert_eq!(
        doc_track_note_count(&editor, 1),
        1,
        "卷帘音符应落到走带锚点轨 doc 1"
    );
    assert_eq!(
        doc_track_note_count(&editor, 0),
        1,
        "doc 0 仅有原始种子，不应被误写入"
    );
}

/// 卷帘复制 → 走带粘贴（JSON）：卷侧 JSON 无 `type` 字段，须被走带接受
#[test]
fn test_piano_roll_json_pastes_into_arrangement() {
    let mut editor = Editor::default();
    seed_notes(&mut editor, 3, 0, &[Note::from_raw(0.0, 60, 10.0, 100, 0)]);
    editor.selection_insert(0);

    let json = editor
        .build_clipboard_json(0, 480)
        .expect("卷帘 JSON 编码失败");
    assert!(
        !json.contains("\"type\""),
        "卷帘 JSON 不带 type 字段（走带格式才带）"
    );

    editor
        .editor_state
        .data
        .arrange_selection
        .add_rect_track(0, 10, 0, 127, 2, 2);
    assert!(
        editor.arrange_paste_from_text(&json),
        "卷帘 JSON 载荷必须能被走带粘贴（原缺陷：静默失败）"
    );
    assert_eq!(
        doc_track_note_count(&editor, 2),
        1,
        "卷帘音符应落到走带锚点轨 doc 2"
    );
}

/// 走带复制 → 卷帘粘贴（二进制）：per-note track 偏移必须被路由到不同音轨
///
/// 原缺陷：卷帘把 track 字段丢弃，全部音符叠进当前轨——看起来成功、结果全错。
#[test]
fn test_arrangement_binary_pastes_multi_track_into_piano_roll() {
    let mut editor = Editor::default();
    seed_notes(&mut editor, 3, 0, &[]);
    // 视觉轨 0 与 1 各一个音符 → track 偏移 0 / 1
    let all_notes = vec![
        (0usize, NoteEvent::new(0, 10, 60, 100, 0)),
        (1usize, NoteEvent::new(0, 10, 64, 100, 0)),
    ];
    let bytes = editor
        .encode_arrangement_clipboard_binary(&all_notes)
        .expect("走带二进制编码失败");

    assert!(
        editor.paste_binary_payload(&bytes),
        "走带载荷必须能被卷帘粘贴"
    );
    assert_eq!(
        doc_track_note_count(&editor, 0),
        1,
        "偏移 0 的音符应落当前轨 doc 0"
    );
    assert_eq!(
        doc_track_note_count(&editor, 1),
        1,
        "偏移 1 的音符必须路由到 doc 1（原缺陷：被丢弃后叠进当前轨）"
    );
    assert_eq!(doc_track_note_count(&editor, 2), 0);
}

/// 走带复制 → 卷帘粘贴（JSON）：同样按 track 偏移多轨路由
#[test]
fn test_arrangement_json_pastes_multi_track_into_piano_roll() {
    let mut editor = Editor::default();
    seed_notes(&mut editor, 3, 0, &[]);
    let json = r#"{"type":"arrangement","origin_tick":0.0,"origin_key":60,"origin_track":0,"division":480,"notes":[{"tick":0.0,"key":0,"length":10.0,"velocity":100,"channel":0,"track":0},{"tick":0.0,"key":4,"length":10.0,"velocity":100,"channel":0,"track":1}]}"#;

    assert!(
        editor.paste_json_payload(json),
        "走带 JSON 必须能被卷帘粘贴"
    );
    assert_eq!(doc_track_note_count(&editor, 0), 1, "偏移 0 → doc 0");
    assert_eq!(
        doc_track_note_count(&editor, 1),
        1,
        "偏移 1 → doc 1（原缺陷：被丢弃后叠进当前轨）"
    );
}

/// 粘贴后走带选区必须冻结为**新粘贴的音符**（对齐卷帘 `select_notes_by_params`）
///
/// 原缺陷：粘贴后 `arrange_selection` 保持旧矩形，连续 Ctrl+V 的锚点不可预测。
#[test]
fn test_arrangement_paste_selects_pasted_notes() {
    let mut editor = Editor::default();
    seed_notes(&mut editor, 3, 0, &[]);
    editor
        .editor_state
        .data
        .arrange_selection
        .add_rect_track(0, 10, 0, 127, 0, 0);
    assert!(
        editor.arrange_paste_from_text(&single_track_clipboard_json()),
        "粘贴应成功"
    );

    let selection = &editor.editor_state.data.arrange_selection;
    assert!(
        selection.frozen().is_some(),
        "粘贴后选区应冻结为新粘贴音符（精确集合）"
    );
    let notes = editor.editor_state.data.track_notes(0);
    let pasted = notes
        .iter()
        .find(|n| n.start_tick == 0 && n.key == 64)
        .expect("粘贴的音符（origin_key=64）应存在于 doc 0");
    assert!(
        selection.contains(0, pasted.start_tick, pasted.key),
        "新粘贴的音符应在选区内"
    );
    // 旧选区矩形覆盖的其它音符不得被误纳入
    assert_eq!(
        doc_track_note_count(&editor, 0),
        1,
        "doc 0 应仅有粘贴的 1 个音符"
    );
}
