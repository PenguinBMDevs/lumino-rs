//! 事件浏览器表格行聚合 — 自动化事件（CC/PB/RPN/NRPN/Tempo）。

use lumino_extras::i18n::MainTranslations;
use lumino_note_core::automation::{
    AutomationLane, AutomationTarget as LaneTarget, SegmentShape as LaneShape,
};
use lumino_note_core::event::{AutomationTarget as EventTarget, SegmentShape as EventShape};
use lumino_ui_core::sidebar_event::EditRequest;

use crate::sidebar::event_browser::bar_lookup::BarLookup;
use crate::sidebar::event_browser::detail::{EventBrowserData, EventTableRow, make_jump};
use crate::sidebar::event_browser::table::shape_text;

/// 收集自动化事件行。
///
/// Tempo 目标从 `tempo_points` 取，其余目标从匹配的 automation lane 取。
pub(super) fn collect_auto_rows(
    data: &EventBrowserData<'_>,
    bl: &BarLookup,
    track: u16,
    target: &EventTarget,
    t: &MainTranslations,
) -> Vec<EventTableRow> {
    if *target == EventTarget::Tempo {
        collect_tempo_rows(data, bl, track, t)
    } else if let Some(lane) = find_lane(data, track, target) {
        collect_lane_rows(bl, lane, t)
    } else {
        Vec::new()
    }
}

/// 收集 Tempo 事件行（存储于 tempo_points）。
fn collect_tempo_rows(
    data: &EventBrowserData<'_>,
    bl: &BarLookup,
    track: u16,
    t: &MainTranslations,
) -> Vec<EventTableRow> {
    let _ = track;
    data.tempo_points
        .iter()
        .enumerate()
        .map(|(idx, pt)| {
            let tick = pt.tick as u32;
            let shape = EventShape::Step;
            let cells = vec![
                String::new(),
                tick.to_string(),
                bl.format(tick),
                format!("{:.2}", pt.bpm),
                shape_text(shape, t),
            ];
            let value = pt.bpm as f32;
            let edits = vec![
                None,
                Some(EditRequest::AutoTick { tick, value, shape }),
                Some(EditRequest::AutoTick { tick, value, shape }),
                Some(EditRequest::AutoValue { tick, value, shape }),
                Some(EditRequest::AutoShape { tick, value, shape }),
            ];
            let jumps = vec![
                None,
                make_jump(tick, None),
                make_jump(tick, None),
                make_jump(tick, None),
                make_jump(tick, None),
            ];
            EventTableRow {
                id: idx,
                tick,
                cells,
                cell_edits: edits,
                cell_jumps: jumps,
            }
        })
        .collect()
}

/// 收集自动化 lane 事件行。
fn collect_lane_rows(
    bl: &BarLookup,
    lane: &AutomationLane,
    t: &MainTranslations,
) -> Vec<EventTableRow> {
    lane.events
        .iter()
        .enumerate()
        .map(|(idx, evt)| {
            let tick = evt.tick;
            let value = evt.value as f32;
            let shape = lane_shape_to_event_shape(evt.shape);
            let cells = vec![
                String::new(),
                tick.to_string(),
                bl.format(tick),
                value.to_string(),
                shape_text(shape, t),
            ];
            let edits = vec![
                None,
                Some(EditRequest::AutoTick { tick, value, shape }),
                Some(EditRequest::AutoTick { tick, value, shape }),
                Some(EditRequest::AutoValue { tick, value, shape }),
                Some(EditRequest::AutoShape { tick, value, shape }),
            ];
            let jumps = vec![
                None,
                make_jump(tick, None),
                make_jump(tick, None),
                make_jump(tick, None),
                make_jump(tick, None),
            ];
            EventTableRow {
                id: idx,
                tick,
                cells,
                cell_edits: edits,
                cell_jumps: jumps,
            }
        })
        .collect()
}

/// 在 automation lanes 中查找匹配目标。
fn find_lane<'a>(
    data: &EventBrowserData<'a>,
    track: u16,
    target: &EventTarget,
) -> Option<&'a AutomationLane> {
    data.automation_lanes
        .iter()
        .find(|lane| lane.track == track && target_matches(target, &lane.target))
        .map(|lane| lane.as_ref())
}

/// 事件浏览器目标 ↔ 存储 lane 目标匹配。
fn target_matches(event_target: &EventTarget, lane_target: &LaneTarget) -> bool {
    match (event_target, lane_target) {
        // Tempo 单独存储在 tempo_points，不匹配任何 automation lane。
        (EventTarget::Tempo, _) => false,
        (EventTarget::Cc(a), LaneTarget::CC { controller: b }) => a == b,
        (EventTarget::PitchBend, LaneTarget::PitchBend) => true,
        (EventTarget::Rpn(a), LaneTarget::Rpn { parameter: b }) => a == b,
        (EventTarget::Nrpn(a), LaneTarget::Nrpn { parameter: b }) => a == b,
        _ => false,
    }
}

/// 存储 lane 形状 → 事件浏览器形状（tension → 贝塞尔控制点近似，仅显示用）。
fn lane_shape_to_event_shape(shape: LaneShape) -> EventShape {
    match shape {
        LaneShape::Step => EventShape::Step,
        LaneShape::Curve { tension } => {
            let t = (tension as f32 / 127.0).clamp(-1.0, 1.0);
            let (y1, y2) = if t >= 0.0 {
                (0.0, 0.5 * t)
            } else {
                (-0.5 * t, 1.0)
            };
            EventShape::Curve {
                x1: 0.25,
                y1,
                x2: 0.75,
                y2,
            }
        }
    }
}
