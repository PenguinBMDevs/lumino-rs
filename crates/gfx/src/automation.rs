//! 自动化曲线渲染 — 从 AutomationLane 生成 GPU 实例
//!
//! 从 yinhe 项目移植：将 Step / Curve 插值的事件序列转换为 2px 线段与圆角锚点实例。

use lumino_core::{AutomationLane, SegmentShape};

use crate::cc_bar_renderer::CcBarInstance;

/// 自动化节点（曲线 + 锚点）统一使用的蓝色，与主音轨已放置音符
/// `MAIN_TRACK_NOTE_COLOR`（ui crate note_worker.rs）保持一致，确保视觉统一。
pub const AUTOMATION_NODE_COLOR: [f32; 3] = [0.2, 0.55, 1.0];

/// 曲线子采样像素步长。Linear/Curve 段按此步长采样并连成多条短线。
const CURVE_SUBSAMPLE_PX: f32 = 2.0;
/// 锚点半径（像素）。Pencil/Curve 工具下显示。
const ANCHOR_RADIUS: f32 = 3.0;
/// 线段不透明度。
const LINE_ALPHA: f32 = 0.85;

/// 自动化面板局部视图参数（与 yinhe 的 AutomationPanelView 对应的最小集）。
#[derive(Debug, Clone, Copy)]
pub struct AutomationViewParams {
    /// 面板高度（像素）
    pub panel_height: f32,
    /// 每 tick 对应的像素数
    pub pixels_per_tick: f32,
    /// 水平滚动偏移（像素）
    pub scroll_x: f32,
    /// 左侧键盘/轨道列宽度（像素）
    pub keyboard_width: f32,
    /// 垂直缩放系数。1.0 = 满量程映射到面板高度。
    pub value_zoom: f32,
    /// 垂直滚动偏移（值空间单位）。面板顶部对应的值。
    pub value_scroll: f32,
    /// 面板内容区左上角屏幕 X 坐标
    pub panel_offset_x: f32,
    /// 面板内容区左上角屏幕 Y 坐标
    pub panel_offset_y: f32,
    /// 工具栏高度（像素），数据区在工具栏下方
    pub toolbar_height: f32,
    /// 自动化曲线连线粗细（像素，1-10，默认 2）。
    pub line_thickness: f32,
}

impl AutomationViewParams {
    /// 将 tick 转换为屏幕空间 X 坐标（含滚动、键盘宽度与面板偏移）。
    #[inline]
    pub fn tick_to_x(&self, tick: u32) -> f32 {
        self.panel_offset_x + self.keyboard_width - self.scroll_x
            + tick as f32 * self.pixels_per_tick
    }

    /// 将自动化值转换为屏幕空间 Y 坐标（像素）。
    #[inline]
    pub fn value_to_y(&self, value: f32, max_val: f32) -> f32 {
        let visible_range = max_val / self.value_zoom;
        if visible_range <= 0.0 {
            return self.panel_offset_y + self.toolbar_height;
        }
        let h = self.panel_height - self.toolbar_height;
        self.panel_offset_y + self.toolbar_height + h
            - ((value - self.value_scroll) / visible_range) * h
    }

    /// 将屏幕空间 Y 坐标转换回自动化值。
    #[inline]
    pub fn y_to_value(&self, y: f32, max_val: f32) -> f32 {
        let visible_range = max_val / self.value_zoom;
        if visible_range <= 0.0 {
            return 0.0;
        }
        let h = self.panel_height - self.toolbar_height;
        let local_y = y - self.panel_offset_y - self.toolbar_height;
        self.value_scroll + (1.0 - local_y / h) * visible_range
    }

    /// 根据 max_val 限制 value_scroll 的范围。
    pub fn clamp_value_scroll(&mut self, max_val: f32) {
        let visible_range = max_val / self.value_zoom;
        let max_scroll = (max_val - visible_range).max(0.0);
        self.value_scroll = self.value_scroll.clamp(0.0, max_scroll);
    }
}

/// 一个需要绘制的线段（panel 局部像素坐标）。
struct SegSpan {
    x1: f32,
    y1: f32,
    shape: SegmentShape,
    x2: f32,
    y2: f32,
}

/// 单条线段渲染上下文。
struct SegmentContext {
    /// 起点 X 坐标
    x1: f32,
    /// 起点 Y 坐标
    y1: f32,
    /// 终点 X 坐标
    x2: f32,
    /// 终点 Y 坐标
    y2: f32,
    /// 线段形状
    shape: SegmentShape,
    /// 线条颜色
    color: [f32; 3],
    /// 线条粗细
    thickness: f32,
}

/// 折线采样渲染上下文。
struct PolylineContext {
    /// 起点 X 坐标
    x1: f32,
    /// 起点 Y 坐标
    y1: f32,
    /// 终点 X 坐标
    x2: f32,
    /// 终点 Y 坐标
    y2: f32,
    /// 线条颜色
    color: [f32; 3],
    /// 线条粗细
    thickness: f32,
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
    let visible_events = lane.events_in_range(pad_start, pad_end);
    let mut segs = Vec::new();
    let grid_left_x = view.keyboard_width;

    if visible_events.is_empty() {
        let idx = lane.events.partition_point(|e| e.tick < pad_start);
        let val = if idx > 0 {
            lane.events[idx - 1].value
        } else {
            0
        };
        let y = view.value_to_y(val as f32, max_val);
        if width > grid_left_x {
            segs.push(SegSpan {
                x1: grid_left_x,
                y1: y,
                shape: SegmentShape::Step,
                x2: width,
                y2: y,
            });
        }
        return segs;
    }

    let prev_idx = lane
        .events
        .partition_point(|e| e.tick < visible_events[0].tick);
    let chase_val = if prev_idx > 0 {
        lane.events[prev_idx - 1].value
    } else {
        0
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
        });
    }

    let mut prev_x = first_x;
    let mut prev_y = chase_y;
    let mut prev_shape = SegmentShape::Step;

    for evt in visible_events {
        let x2 = view.tick_to_x(evt.tick);
        let y2 = view.value_to_y(evt.value as f32, max_val);
        segs.push(SegSpan {
            x1: prev_x,
            y1: prev_y,
            shape: prev_shape,
            x2,
            y2,
        });
        prev_shape = evt.shape;
        prev_x = x2;
        prev_y = y2;
    }

    let last_visible_tick = visible_events.last().map_or(pad_end, |e| e.tick);
    let next_idx = lane.events.partition_point(|e| e.tick <= last_visible_tick);
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
                color,
                thickness: lt,
            },
        );
    }

    if show_anchors {
        let visible_events = lane.events_in_range(pad_start, pad_end);
        for evt in visible_events {
            let x = view.tick_to_x(evt.tick);
            let y = view.value_to_y(evt.value as f32, max_val);
            out.push(CcBarInstance::with_props(
                x - ANCHOR_RADIUS,
                y - ANCHOR_RADIUS,
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

/// 渲染单条线段（Step 或 Curve）。
fn render_segment(out: &mut Vec<CcBarInstance>, ctx: &SegmentContext) {
    let dx = ctx.x2 - ctx.x1;
    if dx <= 0.0 {
        // 同一 tick 多事件：只画竖直跳变
        let dy = ctx.y2 - ctx.y1;
        if dy.abs() > 0.0 {
            out.push(CcBarInstance::new(
                ctx.x2 - ctx.thickness * 0.5,
                ctx.y1.min(ctx.y2),
                ctx.thickness,
                dy.abs(),
                [ctx.color[0], ctx.color[1], ctx.color[2], LINE_ALPHA],
            ));
        }
        return;
    }

    match ctx.shape {
        SegmentShape::Step => {
            out.push(CcBarInstance::new(
                ctx.x1,
                ctx.y1 - ctx.thickness * 0.5,
                dx,
                ctx.thickness,
                [ctx.color[0], ctx.color[1], ctx.color[2], LINE_ALPHA],
            ));
            let dy = ctx.y2 - ctx.y1;
            if dy.abs() > 0.0 {
                out.push(CcBarInstance::new(
                    ctx.x2 - ctx.thickness * 0.5,
                    ctx.y1.min(ctx.y2),
                    ctx.thickness,
                    dy.abs(),
                    [ctx.color[0], ctx.color[1], ctx.color[2], LINE_ALPHA],
                ));
            }
        }
        SegmentShape::Curve { .. } => {
            let shape = ctx.shape;
            push_polyline(
                out,
                |t| shape.interpolate(t),
                &PolylineContext {
                    x1: ctx.x1,
                    y1: ctx.y1,
                    x2: ctx.x2,
                    y2: ctx.y2,
                    color: ctx.color,
                    thickness: ctx.thickness,
                },
            );
        }
    }
}

/// 沿两点用插值因子函数子采样并画折线。
fn push_polyline(
    out: &mut Vec<CcBarInstance>,
    factor_fn: impl Fn(f32) -> f32,
    ctx: &PolylineContext,
) {
    let dx = ctx.x2 - ctx.x1;
    let dy = ctx.y2 - ctx.y1;
    let pixel_len = dx.hypot(dy);
    if pixel_len < 1.0 {
        return;
    }
    let steps = ((pixel_len / CURVE_SUBSAMPLE_PX).ceil() as usize).max(1);
    let inv = 1.0 / steps as f32;
    let mut px = ctx.x1;
    let mut py = ctx.y1;
    for i in 1..=steps {
        let t = i as f32 * inv;
        let f = factor_fn(t);
        let nx = ctx.x1 + dx * t;
        let ny = ctx.y1 + dy * f;
        let seg_dx = nx - px;
        let seg_dy = ny - py;
        let len = seg_dx.hypot(seg_dy);
        if len > 0.5 {
            if seg_dx.abs() >= seg_dy.abs() {
                out.push(CcBarInstance::new(
                    px.min(nx),
                    py - ctx.thickness * 0.5,
                    seg_dx.abs().max(ctx.thickness),
                    ctx.thickness,
                    [ctx.color[0], ctx.color[1], ctx.color[2], LINE_ALPHA],
                ));
            } else {
                out.push(CcBarInstance::new(
                    px - ctx.thickness * 0.5,
                    py.min(ny),
                    ctx.thickness,
                    seg_dy.abs().max(ctx.thickness),
                    [ctx.color[0], ctx.color[1], ctx.color[2], LINE_ALPHA],
                ));
            }
        }
        px = nx;
        py = ny;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_core::{AutomationEvent, AutomationTarget};

    fn make_lane(ticks: &[u32], values: &[u16]) -> AutomationLane {
        AutomationLane {
            target: AutomationTarget::CC { controller: 7 },
            track: 0,
            channel: 0,
            events: ticks
                .iter()
                .zip(values.iter())
                .map(|(&t, &v)| AutomationEvent {
                    tick: t,
                    value: v,
                    shape: SegmentShape::Step,
                })
                .collect(),
        }
    }

    #[test]
    fn test_value_to_y_and_back() {
        let view = AutomationViewParams {
            panel_height: 100.0,
            pixels_per_tick: 1.0,
            scroll_x: 0.0,
            keyboard_width: 0.0,
            value_zoom: 1.0,
            value_scroll: 0.0,
            panel_offset_x: 0.0,
            panel_offset_y: 0.0,
            toolbar_height: 28.0,
            line_thickness: 2.0,
        };
        assert!((view.value_to_y(0.0, 127.0) - 100.0).abs() < 1e-3);
        assert!((view.value_to_y(127.0, 127.0) - 28.0).abs() < 1e-3);
        assert!((view.y_to_value(view.value_to_y(64.0, 127.0), 127.0) - 64.0).abs() < 1e-3);
    }

    #[test]
    fn test_build_lane_instances_step() {
        let lane = make_lane(&[0, 100], &[0, 127]);
        let view = AutomationViewParams {
            panel_height: 100.0,
            pixels_per_tick: 1.0,
            scroll_x: 0.0,
            keyboard_width: 0.0,
            value_zoom: 1.0,
            value_scroll: 0.0,
            panel_offset_x: 0.0,
            panel_offset_y: 0.0,
            toolbar_height: 28.0,
            line_thickness: 2.0,
        };
        let mut out = Vec::new();
        build_lane_instances(&mut out, 200.0, &view, &lane, [1.0, 1.0, 1.0], false);
        assert!(!out.is_empty(), "应生成 Step 线段");
    }
}
