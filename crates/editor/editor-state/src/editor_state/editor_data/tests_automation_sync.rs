//! 自动化 lane → `document.control_events` 同步验收测试
//!
//! 对应缺陷（2026-10-17 数据源冗余审计 §一）：编辑只写 `automation_lanes` 副本，
//! 不回写 `doc.control_events`（保存 / 导出 / 非当前轨播放的权威）→ 面板里画的
//! CC/PB 曲线「听得到、存不下来」。
//!
//! 本文件把修复后的契约钉死：
//! - CC / PB 的 Add / Move / Delete / Clear 必须落到 document；
//! - PC / 通道触后 / 复音触后等「编辑器未建模」事件不得被同步误删或改值；
//! - RPN/NRPN lane 不落 document（无对应 kind，播放时展开）；
//! - undo 必须让 document 跟着回退；
//! - 同 tick 保持 CC → PB 顺序（RPN 选择 / DataEntry 需先于 PB 生效）。

use lumino_midi_model::{ChunkedList, PackedControlEvent};
use lumino_note_core::automation::{AutomationEdit, AutomationTarget, SegmentShape};

use super::EditorData;

/// document 中 CC7 / PB 的 kind
const KIND_CC: usize = 0;
const KIND_PC: usize = 1;
const KIND_PB: usize = 2;
const KIND_CHANNEL_AT: usize = 3;

/// 含空文档的 EditorData（track 0）
fn data_with_doc() -> EditorData {
    EditorData::with_notes(0, &[])
}

/// 用给定事件重建 document.control_events（模拟"从文件加载的控制事件"）
fn seed_document(data: &mut EditorData, events: Vec<PackedControlEvent>) {
    data.document
        .as_mut()
        .expect("document 应存在")
        .control_events = ChunkedList::from_sorted(events);
}

/// 添加 CC7 事件
fn add_cc(data: &mut EditorData, tick: u32, value: u16) -> bool {
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::CC { controller: 7 },
        channel: 3,
        tick,
        value,
        shape: SegmentShape::Step,
    })
}

/// 添加 PitchBend 事件（原始值 0..16383）
fn add_pb(data: &mut EditorData, tick: u32, value: u16) -> bool {
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::PitchBend,
        channel: 3,
        tick,
        value,
        shape: SegmentShape::Curve { tension: 0 },
    })
}

// ── 1. CC：增 / 移 / 删 / 清 ─────────────────────────────────────────────

#[test]
fn cc_add_writes_to_document() {
    let mut data = data_with_doc();
    assert!(add_cc(&mut data, 100, 64));

    assert_eq!(
        data.document_control_values(0, KIND_CC, Some(7)),
        vec![(100, 64)],
        "CC 编辑必须落到 document（保存/导出权威）"
    );
}

#[test]
fn cc_move_updates_document() {
    let mut data = data_with_doc();
    add_cc(&mut data, 100, 64);

    let lane_idx = data
        .find_automation_lane(0, &AutomationTarget::CC { controller: 7 })
        .expect("lane 应存在");
    let moved = data.apply_automation_edit(AutomationEdit::Move {
        track_idx: 0,
        lane_idx,
        old_tick: 100,
        old_value: Some(64),
        new_tick: 220,
        new_value: 66,
    });
    assert!(moved, "Move 应返回 true");
    assert_eq!(
        data.document_control_values(0, KIND_CC, Some(7)),
        vec![(220, 66)],
        "Move 后 document 不得残留旧 tick / 旧值"
    );
}

#[test]
fn cc_delete_removes_from_document() {
    let mut data = data_with_doc();
    add_cc(&mut data, 100, 64);
    add_cc(&mut data, 200, 70);
    let lane_idx = data
        .find_automation_lane(0, &AutomationTarget::CC { controller: 7 })
        .expect("lane 应存在");

    assert!(data.apply_automation_edit(AutomationEdit::Delete {
        track_idx: 0,
        lane_idx,
        tick: 100,
    }));
    assert_eq!(
        data.document_control_values(0, KIND_CC, Some(7)),
        vec![(200, 70)],
        "删除的事件必须从 document 消失"
    );
}

#[test]
fn cc_clear_removes_all_of_lane_from_document() {
    let mut data = data_with_doc();
    add_cc(&mut data, 100, 64);
    add_cc(&mut data, 200, 70);
    let lane_idx = data
        .find_automation_lane(0, &AutomationTarget::CC { controller: 7 })
        .expect("lane 应存在");

    assert!(data.apply_automation_edit(AutomationEdit::Clear {
        track_idx: 0,
        lane_idx,
    }));
    assert_eq!(
        data.count_document_control_events(0, KIND_CC, Some(7)),
        0,
        "Clear 后 document 不得残留该 CC 事件"
    );
}

#[test]
fn cc_value_is_clamped_to_midi_range() {
    let mut data = data_with_doc();
    // lane 值域是 u16，CC 上限 127：越界值必须夹紧后再写入 document
    add_cc(&mut data, 10, 999);
    assert_eq!(
        data.document_control_values(0, KIND_CC, Some(7)),
        vec![(10, 127)],
        "CC 值必须夹紧到 0..=127"
    );
}

// ── 2. PitchBend ────────────────────────────────────────────────────────

#[test]
fn pitchbend_add_and_move_roundtrip_raw_value() {
    let mut data = data_with_doc();
    assert!(add_pb(&mut data, 50, 12288));
    assert_eq!(
        data.document_control_values(0, KIND_PB, None),
        vec![(50, 12288)],
        "PB 原始值必须无损落到 document（passthrough 导出依赖它）"
    );

    let lane_idx = data
        .find_automation_lane(0, &AutomationTarget::PitchBend)
        .expect("PB lane 应存在");
    assert!(data.apply_automation_edit(AutomationEdit::Move {
        track_idx: 0,
        lane_idx,
        old_tick: 50,
        old_value: Some(12288),
        new_tick: 90,
        new_value: 4096,
    }));
    assert_eq!(
        data.document_control_values(0, KIND_PB, None),
        vec![(90, 4096)],
        "PB Move 后 document 必须同步"
    );
}

// ── 3. 不得误伤编辑器未建模的事件 ────────────────────────────────────────

#[test]
fn pc_and_aftertouch_are_preserved() {
    let mut data = data_with_doc();
    seed_document(
        &mut data,
        vec![
            PackedControlEvent::program_change(10, 0, 3, 42),
            PackedControlEvent::channel_aftertouch(20, 0, 3, 55),
            // 另一条轨/另一个控制器的 CC 也不得被误删
            PackedControlEvent::control_change(30, 1, 0, 11, 90),
        ],
    );

    add_cc(&mut data, 100, 64);

    assert_eq!(
        data.count_document_control_events(0, KIND_PC, None),
        1,
        "PC 必须原样保留（编辑器未建模，靠透传往返）"
    );
    assert_eq!(
        data.count_document_control_events(0, KIND_CHANNEL_AT, None),
        1,
        "通道触后必须原样保留"
    );
    assert_eq!(
        data.document_control_values(1, KIND_CC, Some(11)),
        vec![(30, 90)],
        "未被 lane 覆盖的轨道/控制器 CC 必须原样保留"
    );
    assert_eq!(
        data.document_control_values(0, KIND_CC, Some(7)),
        vec![(100, 64)],
        "编辑的 CC 应写入"
    );
}

#[test]
fn rpn_lane_is_not_written_to_document() {
    let mut data = data_with_doc();
    // RPN 无对应 kind：只存在于编辑态，播放时展开为 CC 序列
    assert!(data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::Rpn { parameter: 0 },
        channel: 0,
        tick: 10,
        value: 100,
        shape: SegmentShape::Step,
    }));
    assert_eq!(
        data.count_document_control_events(0, KIND_CC, None),
        0,
        "RPN lane 不得污染 document 的 CC 事件"
    );
}

// ── 4. undo 必须让 document 跟着回退 ─────────────────────────────────────

#[test]
fn undo_restores_document_control_events() {
    let mut data = data_with_doc();
    add_cc(&mut data, 100, 64);
    assert_eq!(
        data.document_control_values(0, KIND_CC, Some(7)),
        vec![(100, 64)]
    );

    let lane_idx = data
        .find_automation_lane(0, &AutomationTarget::CC { controller: 7 })
        .expect("lane 应存在");

    // 编辑前压历史，再移动到新 tick
    data.push_history();
    assert!(data.apply_automation_edit(AutomationEdit::Move {
        track_idx: 0,
        lane_idx,
        old_tick: 100,
        old_value: Some(64),
        new_tick: 300,
        new_value: 20,
    }));
    assert_eq!(
        data.document_control_values(0, KIND_CC, Some(7)),
        vec![(300, 20)]
    );

    assert!(data.undo(), "undo 应成功");
    assert_eq!(
        data.document_control_values(0, KIND_CC, Some(7)),
        vec![(100, 64)],
        "undo 后 document 必须回退到编辑前的 CC（否则保存仍写出被撤销的曲线）"
    );
}

// ── 5. 同 tick 顺序契约与无文档边界 ───────────────────────────────────────

#[test]
fn same_tick_cc_precedes_pitchbend() {
    let mut data = data_with_doc();
    // 故意先写 PB 再写同 tick 的 CC：重建后顺序必须是 CC → PB
    add_pb(&mut data, 480, 8192);
    add_cc(&mut data, 480, 100);

    let doc = data.document.as_ref().expect("document 应存在");
    let kinds: Vec<usize> = doc
        .control_events
        .iter()
        .filter(|e| e.tick == 480)
        .map(|e| e.kind as usize)
        .collect();
    assert_eq!(
        kinds,
        vec![KIND_CC, KIND_PB],
        "同 tick 必须 CC 先于 PB（RPN 选择/DataEntry 生效契约）"
    );
}

#[test]
fn sync_without_document_is_noop() {
    let mut data = EditorData::new();
    assert!(data.document.is_none());
    // 无文档时编辑仍作用于 lane（与既有行为一致），且不得 panic
    assert!(add_cc(&mut data, 100, 64));
    assert_eq!(data.automation_lanes.len(), 1);
    assert_eq!(
        data.automation_lanes[0].events.len(),
        1,
        "无文档时 lane 仍应记录编辑（document 为空，无处同步）"
    );
}

// ── 6. 形状/控制柄是编辑态装饰：不得改写 document（也避免每帧重建） ──────

#[test]
fn shape_and_handle_edits_keep_document_values() {
    let mut data = data_with_doc();
    add_cc(&mut data, 100, 64);
    add_cc(&mut data, 200, 70);
    let before = data.document_control_values(0, KIND_CC, Some(7));
    let lane_idx = data
        .find_automation_lane(0, &AutomationTarget::CC { controller: 7 })
        .expect("lane 应存在");

    // CycleShape：只切曲线形状（document 无对应字段，导出按 Step 采样）
    assert!(data.apply_automation_edit(AutomationEdit::CycleShape {
        track_idx: 0,
        lane_idx,
        tick: 100,
    }));
    assert_eq!(
        data.document_control_values(0, KIND_CC, Some(7)),
        before,
        "形状切换不得改写 document 内容"
    );

    // UpdateHandles：只改贝塞尔柄
    assert!(data.apply_automation_edit(AutomationEdit::UpdateHandles {
        track_idx: 0,
        lane_idx,
        tick: 100,
        out_handle: (10.0, 5.0),
        in_handle: (-10.0, -5.0),
    }));
    assert_eq!(
        data.document_control_values(0, KIND_CC, Some(7)),
        before,
        "拖控制柄不得改写 document 内容"
    );

    // 形状仍保留在编辑态（供 UI 绘制与实时播放采样）
    assert!(
        matches!(
            data.automation_lanes[lane_idx].events[0].shape,
            SegmentShape::Curve { .. }
        ),
        "形状必须保留在 lane（仅是不落 document）"
    );
}

#[test]
fn same_tick_value_move_updates_document_value() {
    let mut data = data_with_doc();
    add_cc(&mut data, 100, 64);
    let lane_idx = data
        .find_automation_lane(0, &AutomationTarget::CC { controller: 7 })
        .expect("lane 应存在");

    // 同 tick 仅改值（弯音拖动 tick 锁定路径的 CC 等价场景）
    assert!(data.apply_automation_edit(AutomationEdit::Move {
        track_idx: 0,
        lane_idx,
        old_tick: 100,
        old_value: Some(64),
        new_tick: 100,
        new_value: 127,
    }));
    assert_eq!(
        data.document_control_values(0, KIND_CC, Some(7)),
        vec![(100, 127)],
        "同 tick 改值也必须同步到 document"
    );
}

// ── 7. 性能口径诊断（默认 ignore） ───────────────────────────────────────
//
// 收口「同步是全量重建」这条遗留必须靠**测量**而不是估算：
// `cargo test -p lumino-editor-state --lib sync_cost_diag -- --ignored --nocapture`

#[test]
#[ignore = "性能口径诊断，按需手动运行（--ignored --nocapture）"]
fn sync_cost_diag() {
    let mut data = data_with_doc();

    // 10 万条「不属于 lane 覆盖范围」的既有控制事件（track 1、多控制器）：
    // 模拟黑乐谱加载后 `document.control_events` 的规模
    let events: Vec<PackedControlEvent> = (0..100_000u32)
        .map(|i| PackedControlEvent::control_change(i, 1, 0, (i % 120) as u8, (i % 128) as u8))
        .collect();
    seed_document(&mut data, events);

    // track 0 的 CC7 lane：1000 个锚点
    for i in 0..1000u32 {
        add_cc(&mut data, 1000 + i * 2, (i % 128) as u16);
    }

    let iters = 20u32;

    // 1) 仅同步成本
    let t_sync = std::time::Instant::now();
    for _ in 0..iters {
        data.sync_automation_to_document();
    }
    let per_sync = t_sync.elapsed() / iters;

    // 2) 完整编辑成本（lane 更新 + 自动柄重算 + 同步）= 拖锚点的每帧成本
    let t_edit = std::time::Instant::now();
    for i in 0..iters {
        add_cc(&mut data, 900_000 + i, 64);
    }
    let per_edit = t_edit.elapsed() / iters;

    let doc_events = data
        .document
        .as_ref()
        .expect("document 应存在")
        .control_events
        .len();
    println!(
        "sync_cost_diag: doc_events={doc_events} lane_events={} | sync-only={per_sync:?} | full-edit={per_edit:?}",
        data.automation_lanes[0].events.len()
    );
}
