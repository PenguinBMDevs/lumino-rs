//! CC / Bend 自动化 lane 快照：生产状态 → egui 模型。
//!
//! 拆出本文件以保持 `egui_chrome/velocity_panel.rs` < 400 行。

use lumino_editor_state::EditorData;
use lumino_ui_egui::velocity_panel::{AutomationAnchor, AutomationLaneModel};

/// 依当前编辑模式构建自动化 lane 快照；`bend = false` 时按 `selected_cc` 取 CC lane。
pub(crate) fn lane_model(
    data: &EditorData,
    bend: bool,
    selected_cc: u8,
) -> Option<AutomationLaneModel> {
    let target = if bend {
        lumino_note_core::AutomationTarget::PitchBend
    } else {
        lumino_note_core::AutomationTarget::CC {
            controller: selected_cc,
        }
    };
    let track_idx = data.current_track as u16;
    let lane_idx = data.find_automation_lane(track_idx, &target);
    let anchors = lane_idx
        .and_then(|idx| data.automation_lanes.get(idx))
        .map(|lane| {
            lane.events
                .iter()
                .map(|event| AutomationAnchor {
                    tick: event.tick,
                    value: event.value,
                })
                .collect()
        })
        .unwrap_or_default();
    let (controller, max_value) = if bend {
        (None, 16383.0)
    } else {
        (Some(selected_cc), 127.0)
    };
    Some(AutomationLaneModel {
        track_idx,
        lane_idx,
        controller,
        max_value,
        anchors,
    })
}
