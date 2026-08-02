//! 事件浏览器左侧树的 automation lanes 叶子。
//!
//! 从 `AutomationLane` 列表为每条 lane 生成 `TreeItem::Leaf`，
//! 与 `crates/editor-state/.../shape_convert.rs` 的映射保持互逆。

use std::sync::Arc;

use lumino_note_core::automation::AutomationLane;
use lumino_note_core::event::AutomationTarget;

use crate::sidebar::event_browser::state::{SelectedItem, TreeItem};

/// 收集指定音轨的 automation lanes 叶子。
///
/// 每条 lane 对应一个 `SelectedItem::Automation` 叶子，名称取自
/// `AutomationTarget::display_name()`（如 "CC 7 (Volume)"、"Pitch Bend"）。
/// Tempo 目标不存在于 lanes（单独存于 tempo_points，树中为 Conductor 固定项）。
pub(super) fn collect_automation_items(
    track_idx: u16,
    lanes: &[Arc<AutomationLane>],
) -> Vec<TreeItem> {
    lanes
        .iter()
        .filter(|lane| lane.track == track_idx)
        .map(|lane| TreeItem::Leaf {
            name: lane.target.display_name(),
            depth: 3,
            item: SelectedItem::Automation {
                track: track_idx,
                target: lane_target_to_event_target(&lane.target),
            },
        })
        .collect()
}

/// 存储 lane 目标 → 事件浏览器目标（与 `detail/auto.rs::target_matches` 互逆）。
fn lane_target_to_event_target(
    target: &lumino_note_core::automation::AutomationTarget,
) -> AutomationTarget {
    use lumino_note_core::automation::AutomationTarget as LaneTarget;
    match target {
        LaneTarget::CC { controller } => AutomationTarget::Cc(*controller),
        LaneTarget::PitchBend => AutomationTarget::PitchBend,
        LaneTarget::Rpn { parameter } => AutomationTarget::Rpn(*parameter),
        LaneTarget::Nrpn { parameter } => AutomationTarget::Nrpn(*parameter),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_lane(
        track: u16,
        target: lumino_note_core::automation::AutomationTarget,
    ) -> Arc<AutomationLane> {
        Arc::new(AutomationLane {
            target,
            track,
            channel: 0,
            events: Vec::new(),
        })
    }

    #[test]
    fn collect_automation_items_filters_by_track() {
        use lumino_note_core::automation::AutomationTarget as LaneTarget;
        let lanes = vec![
            make_lane(0, LaneTarget::CC { controller: 7 }),
            make_lane(0, LaneTarget::PitchBend),
            make_lane(1, LaneTarget::Rpn { parameter: 0 }), // 其他音轨，不应出现
        ];
        let items = collect_automation_items(0, &lanes);
        assert_eq!(items.len(), 2);
        assert!(items.iter().any(|i| matches!(
            i,
            TreeItem::Leaf {
                name,
                item: SelectedItem::Automation {
                    track: 0,
                    target: AutomationTarget::Cc(7),
                },
                ..
            } if name == "CC 7 (Volume)"
        )));
        assert!(items.iter().any(|i| matches!(
            i,
            TreeItem::Leaf {
                item: SelectedItem::Automation {
                    track: 0,
                    target: AutomationTarget::PitchBend,
                },
                ..
            }
        )));
        // 所有叶子 depth = 3（Track 下）
        assert!(
            items
                .iter()
                .all(|i| matches!(i, TreeItem::Leaf { depth: 3, .. }))
        );
    }

    #[test]
    fn lane_target_mapping_roundtrip() {
        use lumino_note_core::automation::AutomationTarget as LaneTarget;
        let cases = [
            (LaneTarget::CC { controller: 64 }, AutomationTarget::Cc(64)),
            (LaneTarget::PitchBend, AutomationTarget::PitchBend),
            (LaneTarget::Rpn { parameter: 1 }, AutomationTarget::Rpn(1)),
            (LaneTarget::Nrpn { parameter: 2 }, AutomationTarget::Nrpn(2)),
        ];
        for (lane, event) in cases {
            assert_eq!(lane_target_to_event_target(&lane), event);
        }
    }

    #[test]
    fn empty_lanes_produce_no_items() {
        assert!(collect_automation_items(0, &[]).is_empty());
    }
}
