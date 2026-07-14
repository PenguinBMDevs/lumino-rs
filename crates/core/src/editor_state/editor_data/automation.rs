//! 自动化事件操作 —— CC/Bend/RPN/NRPN 的 lane 管理、编辑与导出

use super::EditorData;
use crate::automation::{AutomationEdit, AutomationLane, SegmentShape};

impl EditorData {
    /// 查找指定 track + target 的 automation lane 索引。
    pub fn find_automation_lane(
        &self,
        track: u16,
        target: &crate::automation::AutomationTarget,
    ) -> Option<usize> {
        self.automation_lanes
            .iter()
            .position(|l| l.track == track && &l.target == target)
    }

    /// 查找或创建指定 track + target 的 automation lane，返回其索引。
    pub fn find_or_create_automation_lane(
        &mut self,
        track: u16,
        target: crate::automation::AutomationTarget,
    ) -> usize {
        if let Some(idx) = self.find_automation_lane(track, &target) {
            return idx;
        }
        let idx = self.automation_lanes.len();
        self.automation_lanes.push(AutomationLane {
            target,
            track,
            events: Vec::new(),
        });
        idx
    }

    /// 应用单个自动化编辑操作到数据模型。
    ///
    /// 返回是否实际修改了数据。
    pub fn apply_automation_edit(&mut self, edit: AutomationEdit) -> bool {
        match edit {
            AutomationEdit::Add {
                track_idx,
                target,
                tick,
                value,
                shape,
            } => {
                let idx = self.find_or_create_automation_lane(track_idx, target);
                let lane = &mut self.automation_lanes[idx];
                // 移除同一 tick 的已有事件，保证唯一性。
                lane.events.retain(|e| e.tick != tick);
                lane.events
                    .push(crate::automation::AutomationEvent { tick, value, shape });
                lane.events.sort_by_key(|e| e.tick);
                true
            }
            AutomationEdit::Move {
                track_idx,
                lane_idx,
                old_tick,
                new_tick,
                new_value,
            } => {
                let Some(lane) = self.automation_lanes.get_mut(lane_idx) else {
                    return false;
                };
                if lane.track != track_idx {
                    return false;
                }
                let Some(pos) = lane.events.iter().position(|e| e.tick == old_tick) else {
                    return false;
                };
                // 若移动到的 tick 已存在其他事件，先移除。
                lane.events.retain(|e| e.tick != new_tick);
                lane.events[pos].tick = new_tick;
                lane.events[pos].value = new_value;
                lane.events.sort_by_key(|e| e.tick);
                true
            }
            AutomationEdit::CycleShape {
                track_idx,
                lane_idx,
                tick,
            } => {
                let Some(lane) = self.automation_lanes.get_mut(lane_idx) else {
                    return false;
                };
                if lane.track != track_idx {
                    return false;
                }
                let Some(evt) = lane.events.iter_mut().find(|e| e.tick == tick) else {
                    return false;
                };
                evt.shape = match evt.shape {
                    SegmentShape::Step => SegmentShape::Curve { tension: 0 },
                    SegmentShape::Curve { .. } => SegmentShape::Step,
                };
                true
            }
            AutomationEdit::Delete {
                track_idx,
                lane_idx,
                tick,
            } => {
                let Some(lane) = self.automation_lanes.get_mut(lane_idx) else {
                    return false;
                };
                if lane.track != track_idx {
                    return false;
                }
                let old_len = lane.events.len();
                lane.events.retain(|e| e.tick != tick);
                lane.events.len() != old_len
            }
        }
    }

    /// 从 automation_lanes 构建当前音轨的 CC 控制点列表（兼容旧渲染管线）。
    pub fn build_cc_points(&self, controller: u8) -> Vec<crate::midi_types::CcPoint> {
        let target = crate::automation::AutomationTarget::CC { controller };
        let track = self.current_track as u16;
        let Some(idx) = self.find_automation_lane(track, &target) else {
            return Vec::new();
        };
        self.automation_lanes[idx]
            .events
            .iter()
            .map(|e| crate::midi_types::CcPoint {
                tick: e.tick as f32,
                value: (e.value as u8).min(127),
            })
            .collect()
    }

    /// 从 automation_lanes 构建当前音轨的弯音控制点列表（兼容旧渲染管线）。
    pub fn build_bend_points(&self) -> Vec<crate::midi_types::BendPoint> {
        let target = crate::automation::AutomationTarget::PitchBend;
        let track = self.current_track as u16;
        let Some(idx) = self.find_automation_lane(track, &target) else {
            return Vec::new();
        };
        self.automation_lanes[idx]
            .events
            .iter()
            .map(|e| crate::midi_types::BendPoint {
                tick: e.tick as f32,
                value: (e.value as i16 - 8192).clamp(-8192, 8191),
            })
            .collect()
    }
}
