//! 贝塞尔自动柄重算与钳制测试
//!
//! 覆盖：
//! - Add/Delete 事件后自动柄重算（1/3 段长直线语义）
//! - UpdateHandles 拖柄标记自定义、后续编辑不覆盖
//! - 越界柄钳制（防曲线回环）：垂直切线钳制 / 相邻锚点钳制

use super::{EditorData, seed_pb};
use lumino_note_core::automation::AutomationEdit;

#[test]
fn test_apply_automation_edit_add_recomputes_auto_handles() {
    // 两个连续事件：自动柄 = 1/3 段长（直线语义）
    let mut data = EditorData::new();
    seed_pb(&mut data, 0, 8192);
    seed_pb(&mut data, 960, 10000);
    let lane = &data.automation_lanes[0];
    assert!(lane.events[0].handles_auto);
    assert_eq!(lane.events[0].out_handle, (320.0, 602.6667));
    assert_eq!(lane.events[1].in_handle, (-320.0, -602.6667));
}

#[test]
fn test_apply_automation_edit_update_handles() {
    let mut data = EditorData::new();
    seed_pb(&mut data, 100, 8192);
    let changed = data.apply_automation_edit(AutomationEdit::UpdateHandles {
        track_idx: 0,
        lane_idx: 0,
        tick: 100,
        out_handle: (300.0, 500.0),
        in_handle: (0.0, 0.0),
    });
    assert!(changed);
    let evt = &data.automation_lanes[0].events[0];
    assert_eq!(evt.out_handle, (300.0, 500.0));
    assert!(!evt.handles_auto, "拖柄后标记为自定义");

    // 后续编辑（Add 相邻事件）不覆盖自定义柄
    seed_pb(&mut data, 960, 9000);
    assert_eq!(
        data.automation_lanes[0].events[0].out_handle,
        (300.0, 500.0)
    );
}

#[test]
fn test_apply_automation_edit_update_handles_clamps_loopback() {
    // 越界柄（出向柄越过锚点垂直切线）→ 应用后被钳制，防曲线回环
    let mut data = EditorData::new();
    seed_pb(&mut data, 100, 8192);
    let changed = data.apply_automation_edit(AutomationEdit::UpdateHandles {
        track_idx: 0,
        lane_idx: 0,
        tick: 100,
        out_handle: (-300.0, 5000.0), // 越界：柄在锚点左侧
        in_handle: (300.0, -5000.0),  // 越界：柄在锚点右侧
    });
    assert!(changed);
    let evt = &data.automation_lanes[0].events[0];
    assert_eq!(evt.out_handle.0, 0.0, "出向柄被钳制在锚点垂直切线");
    assert_eq!(evt.in_handle.0, 0.0, "入向柄被钳制在锚点垂直切线");
    assert_eq!(evt.out_handle.1, 5000.0, "value 偏移不受限");
    assert!(!evt.handles_auto, "拖柄后标记为自定义");
}

#[test]
fn test_apply_automation_edit_update_handles_clamps_to_neighbors() {
    // 出向柄拉到下一锚点之外 / 入向柄拉到上一锚点之外 → 钳制到相邻锚点
    // （防贝塞尔 x(t) 非单调导致曲线回环）
    let mut data = EditorData::new();
    for (tick, value) in [(0u32, 8192u16), (960, 8192), (1920, 9000)] {
        seed_pb(&mut data, tick, value);
    }
    // 事件 0 的出向柄拉到 tick 5000（远超下一事件 960）
    let changed = data.apply_automation_edit(AutomationEdit::UpdateHandles {
        track_idx: 0,
        lane_idx: 0,
        tick: 0,
        out_handle: (5000.0, 4000.0),
        in_handle: (0.0, 0.0),
    });
    assert!(changed);
    assert_eq!(
        data.automation_lanes[0].events[0].out_handle.0, 960.0,
        "出向柄不能越过下一锚点（tick 差 960）"
    );
    assert_eq!(data.automation_lanes[0].events[0].out_handle.1, 4000.0);

    // 事件 2 的入向柄拉到 tick -5000（远超上一事件 960）
    data.apply_automation_edit(AutomationEdit::UpdateHandles {
        track_idx: 0,
        lane_idx: 0,
        tick: 1920,
        out_handle: (0.0, 0.0),
        in_handle: (-5000.0, -3000.0),
    });
    assert_eq!(
        data.automation_lanes[0].events[2].in_handle.0, -960.0,
        "入向柄不能越过上一锚点（tick 差 -960）"
    );
    assert_eq!(data.automation_lanes[0].events[2].in_handle.1, -3000.0);

    // 中间事件 1：两端都钳制
    data.apply_automation_edit(AutomationEdit::UpdateHandles {
        track_idx: 0,
        lane_idx: 0,
        tick: 960,
        out_handle: (5000.0, 0.0),
        in_handle: (-5000.0, 0.0),
    });
    assert_eq!(
        data.automation_lanes[0].events[1].out_handle.0, 960.0,
        "中间锚点出向柄钳制到下一锚点"
    );
    assert_eq!(
        data.automation_lanes[0].events[1].in_handle.0, -960.0,
        "中间锚点入向柄钳制到上一锚点"
    );
}

#[test]
fn test_apply_automation_edit_delete_recomputes_handles() {
    // 三事件：删除中间事件后，首尾事件自动柄重算为新的 1/3 段
    let mut data = EditorData::new();
    for (tick, value) in [(0u32, 8192u16), (480, 9000), (960, 10000)] {
        seed_pb(&mut data, tick, value);
    }
    data.apply_automation_edit(AutomationEdit::Delete {
        track_idx: 0,
        lane_idx: 0,
        tick: 480,
    });
    let lane = &data.automation_lanes[0];
    assert_eq!(lane.events.len(), 2);
    assert_eq!(lane.events[0].out_handle, (320.0, 602.6667));
}
