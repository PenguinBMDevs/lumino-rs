//! 自动化线段收集与实例生成

use super::AutomationViewParams;
use super::draw::{SegmentContext, render_segment};
use crate::cc_bar_renderer::CcBarInstance;
use lumino_note_core::automation::{AutomationEvent, AutomationLane, SegmentShape};

/// 锚点半径（像素）。自动化锚点由 Curve 工具编辑。
const ANCHOR_RADIUS: f32 = 3.0;

/// 一个需要绘制的线段（panel 局部像素坐标）。
struct SegSpan {
    x1: f32,
    y1: f32,
    shape: SegmentShape,
    x2: f32,
    y2: f32,
    /// 贝塞尔控制柄（屏幕坐标）：前事件出向柄 + 后事件入向柄。
    /// 仅两端事件存在自定义柄时填充（None = 按 shape 插值渲染）。
    cp1: Option<(f32, f32)>,
    cp2: Option<(f32, f32)>,
}

/// 收集 lane 在可见范围内的所有线段。
///
/// - chase 段：从网格左边缘到第一个可见事件（保持 chase 值）
/// - event 段：事件之间（用前一个事件的 shape）
/// - right 段：从最后一个事件到右边界
fn collect_segments(
    lane: &AutomationLane,
    view: &AutomationViewParams,
    max_val: f32,
    width: f32,
    pad_start: u32,
    pad_end: u32,
) -> Vec<SegSpan> {
    // 可见事件窗口扩展：贝塞尔控制柄可能把曲线延伸到事件范围之外
    // （柄 tick 偏移越大延伸越远）。若只按事件 tick 裁剪，锚点（事件）
    // 都在视口外但曲线穿过视口时，段不会生成 → 曲线消失/变成水平线。
    let handle_buffer = lane
        .events
        .iter()
        .map(|e| e.out_handle.0.abs().max(e.in_handle.0.abs()))
        .fold(0.0_f32, f32::max);
    let pad_start = pad_start.saturating_sub(handle_buffer.ceil() as u32);
    let pad_end = pad_end.saturating_add(handle_buffer.ceil() as u32);

    let visible_events = lane.events_in_range(pad_start, pad_end);
    let mut segs = Vec::new();
    let grid_left_x = view.keyboard_width;

    if visible_events.is_empty() {
        let idx = lane.events.partition_point(|event| event.tick < pad_start);
        // 无前事件时使用目标默认值：PitchBend 默认 8192（弯音 0 = 居中）
        let val = if idx > 0 {
            lane.events[idx - 1].value
        } else {
            lane.target.default_value()
        };
        let screen_y = view.value_to_y(val as f32, max_val);
        if width > grid_left_x {
            segs.push(SegSpan {
                x1: grid_left_x,
                y1: screen_y,
                shape: SegmentShape::Step,
                x2: width,
                y2: screen_y,
                cp1: None,
                cp2: None,
            });
        }
        return segs;
    }

    let prev_idx = lane
        .events
        .partition_point(|event| event.tick < visible_events[0].tick);
    // 无前事件时使用目标默认值（PitchBend → 8192 居中）
    let chase_val = if prev_idx > 0 {
        lane.events[prev_idx - 1].value
    } else {
        lane.target.default_value()
    };
    let first_tick = visible_events[0].tick;
    let first_x = view.tick_to_x(first_tick);
    let chase_y = view.value_to_y(chase_val as f32, max_val);

    if first_x > grid_left_x {
        segs.push(SegSpan {
            x1: grid_left_x,
            y1: chase_y,
            shape: SegmentShape::Step,
            x2: first_x,
            y2: chase_y,
            cp1: None,
            cp2: None,
        });
    }

    let mut prev_x = first_x;
    let mut prev_y = chase_y;
    let mut prev_shape = SegmentShape::Step;
    let mut prev_evt: Option<AutomationEvent> = None;

    for evt in visible_events {
        let x2 = view.tick_to_x(evt.tick);
        let y2 = view.value_to_y(evt.value as f32, max_val);
        // 贝塞尔控制柄：两端事件任一为自定义柄 → 携带柄渲染（否则按 shape）
        let (cp1, cp2) = match prev_evt {
            Some(prev) if !prev.handles_auto || !evt.handles_auto => {
                let p1 = prev.out_handle_abs();
                let p2 = evt.in_handle_abs();
                (
                    Some((
                        view.tick_to_x(p1.0.round() as u32),
                        view.value_to_y(p1.1, max_val),
                    )),
                    Some((
                        view.tick_to_x(p2.0.round() as u32),
                        view.value_to_y(p2.1, max_val),
                    )),
                )
            }
            _ => (None, None),
        };
        segs.push(SegSpan {
            x1: prev_x,
            y1: prev_y,
            shape: prev_shape,
            x2,
            y2,
            cp1,
            cp2,
        });
        prev_shape = evt.shape;
        prev_x = x2;
        prev_y = y2;
        prev_evt = Some(*evt);
    }

    let last_visible_tick = visible_events.last().map_or(pad_end, |event| event.tick);
    let next_idx = lane
        .events
        .partition_point(|event| event.tick <= last_visible_tick);
    let right_bound = if next_idx < lane.events.len() {
        view.tick_to_x(lane.events[next_idx].tick)
    } else {
        width
    };
    if right_bound > prev_x {
        segs.push(SegSpan {
            x1: prev_x,
            y1: prev_y,
            shape: SegmentShape::Step,
            x2: right_bound,
            y2: prev_y,
            cp1: None,
            cp2: None,
        });
    }

    segs
}

/// 将 lane 渲染为 CcBarInstance 列表。
///
/// `show_anchors` 控制是否绘制事件锚点。
/// 线条粗细使用 `view.line_thickness`。
pub fn build_lane_instances(
    out: &mut Vec<CcBarInstance>,
    width: f32,
    view: &AutomationViewParams,
    lane: &AutomationLane,
    color: [f32; 3],
    show_anchors: bool,
) {
    let target = &lane.target;
    let max_val = target.max_value() as f32;
    if max_val <= 0.0 {
        return;
    }

    let (tick_start, tick_end) = visible_tick_range(width, view);
    let pad_start = tick_start.max(0.0) as u32;
    let pad_end = tick_end.max(0.0) as u32;

    let lt = view.line_thickness;
    let segs = collect_segments(lane, view, max_val, width, pad_start, pad_end);
    for seg in &segs {
        render_segment(
            out,
            &SegmentContext {
                x1: seg.x1,
                y1: seg.y1,
                x2: seg.x2,
                y2: seg.y2,
                shape: seg.shape,
                cp1: seg.cp1,
                cp2: seg.cp2,
                color,
                thickness: lt,
            },
        );
    }

    if show_anchors {
        let visible_events = lane.events_in_range(pad_start, pad_end);
        for evt in visible_events {
            let screen_x = view.tick_to_x(evt.tick);
            let screen_y = view.value_to_y(evt.value as f32, max_val);
            out.push(CcBarInstance::with_props(
                screen_x - ANCHOR_RADIUS,
                screen_y - ANCHOR_RADIUS,
                2.0 * ANCHOR_RADIUS,
                2.0 * ANCHOR_RADIUS,
                [color[0], color[1], color[2], 1.0],
                ANCHOR_RADIUS,
                0.0,
            ));
        }
    }
}

/// 计算当前视图可见的 tick 范围。
fn visible_tick_range(width: f32, view: &AutomationViewParams) -> (f32, f32) {
    let start = view.scroll_x / view.pixels_per_tick;
    let end = (view.scroll_x + width) / view.pixels_per_tick;
    (start, end)
}
