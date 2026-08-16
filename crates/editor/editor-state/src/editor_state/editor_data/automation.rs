//! 自动化事件操作 —— CC/Bend/RPN/NRPN 的 lane 管理、编辑与导出

use super::EditorData;
use lumino_note_core::automation::{
    AutomationEdit, AutomationLane, AutomationTarget, SegmentShape,
};
use std::sync::Arc;

impl EditorData {
    /// 查找指定 track + target 的 automation lane 索引。
    pub fn find_automation_lane(
        &self,
        track: u16,
        target: &lumino_note_core::automation::AutomationTarget,
    ) -> Option<usize> {
        self.automation_lanes
            .iter()
            .position(|l| l.track == track && &l.target == target)
    }

    /// 查找或创建指定 track + target 的 automation lane，返回其索引。
    pub fn find_or_create_automation_lane(
        &mut self,
        track: u16,
        target: lumino_note_core::automation::AutomationTarget,
    ) -> usize {
        if let Some(idx) = self.find_automation_lane(track, &target) {
            return idx;
        }
        let idx = self.automation_lanes.len();
        self.automation_lanes.push(Arc::new(AutomationLane {
            target,
            track,
            channel: 0,
            events: Vec::new(),
        }));
        idx
    }

    /// 应用单个自动化编辑操作到数据模型。
    ///
    /// 返回是否实际修改了数据。
    /// 增删移事件后自动重算自动控制柄（未弯曲段保持直线），
    /// 保证弯音面板贝塞尔路径与卷帘曲线工具语义一致。
    pub fn apply_automation_edit(&mut self, edit: AutomationEdit) -> bool {
        match edit {
            AutomationEdit::Add {
                track_idx,
                target,
                channel,
                tick,
                value,
                shape,
            } => {
                // 弯音跳变对（同 tick 两事件形成直角突变）：PitchBend 允许
                // 同 tick 多事件；其他 target（CC 等）保持同 tick 唯一（替换）。
                // 排序为稳定排序，同 tick 保持创建顺序（与本地 anchors 一致）。
                let is_pitchbend = target == AutomationTarget::PitchBend;
                let idx = self.find_or_create_automation_lane(track_idx, target);
                let lane = Arc::make_mut(&mut self.automation_lanes[idx]);
                // 如果 lane 尚未设置 channel，更新为事件的 channel
                lane.channel = channel;
                if !is_pitchbend {
                    lane.events.retain(|e| e.tick != tick);
                }
                lane.events
                    .push(lumino_note_core::automation::AutomationEvent::new(
                        tick, value, shape,
                    ));
                lane.events.sort_by_key(|e| e.tick);
                lane.recompute_auto_handles();
                true
            }
            AutomationEdit::Move {
                track_idx,
                lane_idx,
                old_tick,
                old_value,
                new_tick,
                new_value,
            } => {
                let Some(arc_lane) = self.automation_lanes.get_mut(lane_idx) else {
                    return false;
                };
                let lane = Arc::make_mut(arc_lane);
                if lane.track != track_idx {
                    return false;
                }
                // 精确匹配：弯音跳变对（同 tick 两事件）用 old_value 区分
                // 目标；其他场景仅按 tick（同 tick 唯一）
                let Some(pos) = lane
                    .events
                    .iter()
                    .position(|e| e.tick == old_tick && old_value.is_none_or(|v| e.value == v))
                else {
                    return false;
                };
                if new_tick == old_tick {
                    // 同 tick 移动（弯音拖动 tick 锁定）：原地更新 value，
                    // 顺序不变（跳变对组内顺序保持）
                    lane.events[pos].value = new_value;
                    lane.recompute_auto_handles();
                    return true;
                }
                // 先取出旧事件（避免 retain 删除自身导致索引越界），
                // 再移除目标 tick 的冲突事件，最后写回。
                let mut evt = lane.events.remove(pos);
                evt.tick = new_tick;
                evt.value = new_value;
                // 弯音跳变对：移动不替换同 tick 已有事件（保持跳变对）；
                // 其他 target（CC 等）保持同 tick 唯一（替换）
                if lane.target != AutomationTarget::PitchBend {
                    lane.events.retain(|e| e.tick != new_tick);
                }
                // 插入到排序位置：同 tick 组末尾（作为组内后建事件）
                let insert_pos = lane.events.partition_point(|e| e.tick <= new_tick);
                lane.events.insert(insert_pos, evt);
                lane.recompute_auto_handles();
                true
            }
            AutomationEdit::CycleShape {
                track_idx,
                lane_idx,
                tick,
            } => {
                let Some(arc_lane) = self.automation_lanes.get_mut(lane_idx) else {
                    return false;
                };
                let lane = Arc::make_mut(arc_lane);
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
                let Some(arc_lane) = self.automation_lanes.get_mut(lane_idx) else {
                    return false;
                };
                let lane = Arc::make_mut(arc_lane);
                if lane.track != track_idx {
                    return false;
                }
                let old_len = lane.events.len();
                lane.events.retain(|e| e.tick != tick);
                let changed = lane.events.len() != old_len;
                if changed {
                    lane.recompute_auto_handles();
                }
                changed
            }
            AutomationEdit::UpdateHandles {
                track_idx,
                lane_idx,
                tick,
                out_handle,
                in_handle,
            } => {
                let Some(arc_lane) = self.automation_lanes.get_mut(lane_idx) else {
                    return false;
                };
                let lane = Arc::make_mut(arc_lane);
                if lane.track != track_idx {
                    return false;
                }
                // 走 setter：钳制柄不越过自身锚点垂直切线（防曲线回环），
                // 并标记自定义（不再被自动重算覆盖）。
                // 弯音跳变对（同 tick 两事件）：更新全部同 tick 事件的柄——
                // 竖直段（span 0）的柄无视觉/播放效果，更新全部保证本地
                // 拖动的柄（可能是同 tick 第二个锚点）在 lane 中生效。
                let mut found = false;
                for evt in lane.events.iter_mut().filter(|e| e.tick == tick) {
                    evt.set_out_handle(out_handle);
                    evt.set_in_handle(in_handle);
                    found = true;
                }
                if !found {
                    return false;
                }
                // 相邻锚点钳制：柄的 tick 不能越过相邻锚点（否则控制柄 x
                // 超出段端点，贝塞尔 x(t) 非单调 → 曲线回环 → 同一 tick
                // 多个弯音值 / 视觉多条曲线）。
                clamp_handles_to_neighbors(lane, tick);
                true
            }
            AutomationEdit::Clear {
                track_idx,
                lane_idx,
            } => {
                let Some(arc_lane) = self.automation_lanes.get_mut(lane_idx) else {
                    return false;
                };
                let lane = Arc::make_mut(arc_lane);
                if lane.track != track_idx {
                    return false;
                }
                let old_len = lane.events.len();
                lane.events.clear();
                lane.events.len() != old_len
            }
        }
    }

    /// 从 automation_lanes 构建当前音轨的 CC 控制点列表（兼容旧渲染管线）。
    pub fn build_cc_points(&self, controller: u8) -> Vec<lumino_note_core::midi_types::CcPoint> {
        let target = lumino_note_core::automation::AutomationTarget::CC { controller };
        let track = self.current_track as u16;
        let Some(idx) = self.find_automation_lane(track, &target) else {
            return Vec::new();
        };
        self.automation_lanes[idx]
            .events
            .iter()
            .map(|event| lumino_note_core::midi_types::CcPoint {
                tick: event.tick as f32,
                value: (event.value as u8).min(127),
            })
            .collect()
    }

    /// 从 automation_lanes 构建当前音轨的弯音控制点列表（兼容旧渲染管线）。
    pub fn build_bend_points(&self) -> Vec<lumino_note_core::midi_types::BendPoint> {
        let target = lumino_note_core::automation::AutomationTarget::PitchBend;
        let track = self.current_track as u16;
        let Some(idx) = self.find_automation_lane(track, &target) else {
            return Vec::new();
        };
        self.automation_lanes[idx]
            .events
            .iter()
            .map(|event| lumino_note_core::midi_types::BendPoint {
                tick: event.tick as f32,
                value: (event.value as i16 - 8192).clamp(-8192, 8191),
            })
            .collect()
    }
}

/// 按相邻事件钳制指定事件的控制柄 tick 偏移。
///
/// 规则（防贝塞尔曲线回环）：
/// - 出向柄 tick 偏移 ∈ [0, 下一事件 tick 差]（不能越过自身与下一锚点）；
/// - 入向柄 tick 偏移 ∈ [上一事件 tick 差, 0]（不能越过自身与上一锚点）。
///
/// 满足后控制柄 x 均落在段端点 [A.x, B.x] 内，贝塞尔 x(t) 严格单调，
/// 曲线为单值函数（同一 tick 不会出现多个弯音值）。
fn clamp_handles_to_neighbors(lane: &mut lumino_note_core::automation::AutomationLane, tick: u32) {
    let Some(pos) = lane.events.iter().position(|e| e.tick == tick) else {
        return;
    };
    let evt_tick = lane.events[pos].tick as f32;
    // 出向柄：不能超过下一事件
    if let Some(next) = lane.events.get(pos + 1) {
        let max_x = (next.tick as f32 - evt_tick).max(0.0);
        lane.events[pos].out_handle.0 = lane.events[pos].out_handle.0.clamp(0.0, max_x);
    }
    // 入向柄：不能超过上一事件
    if pos > 0 {
        let prev_tick = lane.events[pos - 1].tick as f32;
        let min_x = (prev_tick - evt_tick).min(0.0);
        lane.events[pos].in_handle.0 = lane.events[pos].in_handle.0.clamp(min_x, 0.0);
    }
}
