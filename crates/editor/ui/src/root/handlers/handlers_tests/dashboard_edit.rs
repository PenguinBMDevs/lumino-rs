//! 工具栏仪表盘字段编辑契约（BPM / PPQ）：失焦收口与提交语义
//!
//! 从 `document_sync.rs` 拆出（REF-001：单文件 ≤600 行）。

use super::*;

// ── PPQ 编辑提交契约（点击空白处保存） ───────────────────────────────

/// PPQ 编辑只由输入框失焦（`PpqEditConfirmed`）收口。
///
/// 回归：`Root::update` 曾按「任意非 PPQ 消息」自动确认，但生产分发走
/// `Host::route_message`，该兜底只在测试里生效——测试会断言一个生产不存在
/// 的行为。编辑态与缓冲区必须原样保留，等真正的失焦信号到达。
#[test]
fn test_ppq_edit_ignores_unrelated_messages() {
    let mut root = create_root();
    root.toolbar.ppq_editing = true;
    root.toolbar.ppq_edit_buffer = "480*2".to_owned();
    let before = root.editor.editor_state.view.ppq;

    root.update(crate::toolbar::Event::button_hovered(None));

    assert!(root.toolbar.ppq_editing, "无关消息不得结束 PPQ 编辑态");
    assert_eq!(
        root.toolbar.ppq_edit_buffer, "480*2",
        "用户输入必须原样保留"
    );
    assert_eq!(
        root.editor.editor_state.view.ppq, before,
        "未失焦前不得提前写入 PPQ"
    );
}

/// 失焦提交与回车同口径：整数表达式经 `eval_int_field` 求值后写入。
#[test]
fn test_ppq_edit_confirmed_applies_expression() {
    let mut root = create_root();
    root.toolbar.ppq_editing = true;
    root.toolbar.ppq_edit_buffer = "480*2".to_owned();

    root.update(crate::toolbar::Event::ppq_edit_confirmed());

    assert!(!root.toolbar.ppq_editing, "确认后必须退出编辑态");
    assert!(
        root.toolbar.ppq_edit_buffer.is_empty(),
        "确认后必须清空编辑缓冲"
    );
    assert_eq!(
        root.editor.editor_state.view.ppq, 960,
        "表达式 480*2 应求值为 960"
    );
}

// ── BPM 编辑提交契约（点击 BPM 行 → 修改当前 tempo 点） ──────────────

/// 确认 BPM：只修改**进入编辑时显示**的 tempo 点，其它点不动，并进入撤销栈。
#[test]
fn test_bpm_edit_updates_active_tempo_point() {
    use lumino_note_core::midi_types::TempoPoint;

    let mut root = create_root();
    root.editor.editor_state.data.set_tempo_points(vec![
        TempoPoint {
            tick: 0.0,
            bpm: 120.0,
        },
        TempoPoint {
            tick: 4800.0,
            bpm: 90.0,
        },
    ]);
    root.editor.playback_position = 5000.0;
    root.update(crate::toolbar::Event::bpm_edit_toggled(90.0));
    assert_eq!(root.toolbar.bpm_edit_buffer, "90", "缓冲区应取当前 BPM");
    root.toolbar.bpm_edit_buffer = "100".to_owned();

    root.update(crate::toolbar::Event::bpm_edit_confirmed());

    assert!(!root.toolbar.bpm_editing, "确认后必须退出编辑态");
    let points = &root.editor.editor_state.data.tempo_points;
    assert!(
        (points[0].bpm - 120.0).abs() < 1e-9,
        "更早的 tempo 点不得被修改"
    );
    assert!(
        (points[1].bpm - 100.0).abs() < 1e-9,
        "应修改进入编辑时显示的 tempo 点"
    );
    assert!(
        root.editor.editor_state.data.can_undo(),
        "BPM 修改应进入撤销栈"
    );
}

/// 播放中编辑 BPM：播放头前进后提交，仍改进入编辑时锚定的那条 tempo 点。
#[test]
fn test_bpm_edit_anchor_survives_playback_advance() {
    use lumino_note_core::midi_types::TempoPoint;

    let mut root = create_root();
    root.editor.editor_state.data.set_tempo_points(vec![
        TempoPoint {
            tick: 0.0,
            bpm: 120.0,
        },
        TempoPoint {
            tick: 4800.0,
            bpm: 90.0,
        },
    ]);
    root.editor.playback_position = 5000.0;
    root.update(crate::toolbar::Event::bpm_edit_toggled(90.0));
    assert_eq!(root.toolbar.bpm_edit_anchor_tick, 5000.0);

    // 编辑期间播放头退回到前一个速度点之前。
    root.editor.playback_position = 100.0;
    root.toolbar.bpm_edit_buffer = "100".to_owned();
    root.update(crate::toolbar::Event::bpm_edit_confirmed());

    let points = &root.editor.editor_state.data.tempo_points;
    assert!(
        (points[0].bpm - 120.0).abs() < 1e-9,
        "不得改到提交时刻生效的另一条速度点"
    );
    assert!((points[1].bpm - 100.0).abs() < 1e-9);
}

/// 播放位置在首点之前 / 只有首点时：修改首点（与仪表盘显示同源）。
#[test]
fn test_bpm_edit_at_start_updates_initial_tempo() {
    let mut root = create_root();
    assert_eq!(
        root.editor.editor_state.data.tempo_points.len(),
        1,
        "默认编辑器应有一条 tick 0 的速度点"
    );
    root.editor.playback_position = 0.0;
    root.update(crate::toolbar::Event::bpm_edit_toggled(120.0));
    root.toolbar.bpm_edit_buffer = "140.5".to_owned();

    root.update(crate::toolbar::Event::bpm_edit_confirmed());

    let point = &root.editor.editor_state.data.tempo_points[0];
    assert_eq!(point.tick, 0.0);
    assert!((point.bpm - 140.5).abs() < 1e-9);
    assert!((root.dashboard_bpm() - 140.5).abs() < 1e-9);
}

/// 速度表被删空：确认 BPM 应补一条 tick 0 的显式速度点（而不是静默失败）。
#[test]
fn test_bpm_edit_creates_point_when_tempo_list_empty() {
    let mut root = create_root();
    root.editor.editor_state.data.set_tempo_points(Vec::new());
    root.update(crate::toolbar::Event::bpm_edit_toggled(120.0));
    root.toolbar.bpm_edit_buffer = "90".to_owned();

    root.update(crate::toolbar::Event::bpm_edit_confirmed());

    let points = &root.editor.editor_state.data.tempo_points;
    assert_eq!(points.len(), 1, "空速度表应补一个点");
    assert_eq!(points[0].tick, 0.0);
    assert!((points[0].bpm - 90.0).abs() < 1e-9);
}

/// 越界 / 非数字输入：保留编辑态与错误，绝不写入非法 BPM。
#[test]
fn test_bpm_edit_invalid_keeps_editing_and_value() {
    let mut root = create_root();
    let before = root.editor.editor_state.data.tempo_points[0].bpm;
    root.update(crate::toolbar::Event::bpm_edit_toggled(before));
    root.toolbar.bpm_edit_buffer = "5".to_owned();

    root.update(crate::toolbar::Event::bpm_edit_confirmed());

    assert!(root.toolbar.bpm_editing, "非法输入不得退出编辑态");
    assert!(root.toolbar.bpm_edit_error.is_some(), "必须给出错误提示");
    assert_eq!(root.toolbar.bpm_edit_buffer, "5", "必须保留用户输入");
    assert!(
        (root.editor.editor_state.data.tempo_points[0].bpm - before).abs() < 1e-9,
        "不得写入非法 BPM"
    );
}
