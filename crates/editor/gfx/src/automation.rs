//! 自动化曲线渲染 — 从 AutomationLane 生成 GPU 实例
//!
//! 从 yinhe 项目移植：将 Step / Curve 插值的事件序列转换为 2px 线段与圆角锚点实例。

use lumino_note_core::automation::{AutomationEvent, AutomationLane, SegmentShape};

use crate::cc_bar_renderer::CcBarInstance;

/// 自动化节点（曲线 + 锚点）统一使用的蓝色，与主音轨已放置音符
/// `MAIN_TRACK_NOTE_COLOR`（ui crate note_worker.rs）保持一致，确保视觉统一。
pub const AUTOMATION_NODE_COLOR: [f32; 3] = [0.2, 0.55, 1.0];

/// 曲线子采样像素步长。Linear/Curve 段按此步长采样并连成多条短线。
const CURVE_SUBSAMPLE_PX: f32 = 2.0;
/// 锚点半径（像素）。自动化锚点由 Curve 工具编辑。
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
        let available_height = self.panel_height - self.toolbar_height;
        self.panel_offset_y + self.toolbar_height + available_height
            - ((value - self.value_scroll) / visible_range) * available_height
    }

    /// 将屏幕空间 Y 坐标转换回自动化值。
    #[inline]
    pub fn y_to_value(&self, y: f32, max_val: f32) -> f32 {
        let visible_range = max_val / self.value_zoom;
        if visible_range <= 0.0 {
            return 0.0;
        }
        let available_height = self.panel_height - self.toolbar_height;
        let local_y = y - self.panel_offset_y - self.toolbar_height;
        self.value_scroll + (1.0 - local_y / available_height) * visible_range
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
    /// 贝塞尔控制柄（屏幕坐标）：前事件出向柄 + 后事件入向柄。
    /// 仅两端事件存在自定义柄时填充（None = 按 shape 插值渲染）。
    cp1: Option<(f32, f32)>,
    cp2: Option<(f32, f32)>,
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
    /// 贝塞尔控制柄（屏幕坐标）
    cp1: Option<(f32, f32)>,
    cp2: Option<(f32, f32)>,
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
            if let (Some(cp1), Some(cp2)) = (ctx.cp1, ctx.cp2) {
                // 贝塞尔控制柄：按控制多边形子采样（弯音面板贝塞尔路径）
                push_bezier_polyline(out, ctx, cp1, cp2);
            } else {
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
}

/// 沿三次贝塞尔控制柄子采样并画折线（x/y 均按贝塞尔曲线，非线性插值）
fn push_bezier_polyline(
    out: &mut Vec<CcBarInstance>,
    ctx: &SegmentContext,
    cp1: (f32, f32),
    cp2: (f32, f32),
) {
    // 控制多边形长度估算像素步数（过采样保证曲线平滑）
    let c1 = (cp1.0 - ctx.x1).hypot(cp1.1 - ctx.y1);
    let c2 = (cp2.0 - cp1.0).hypot(cp2.1 - cp1.1);
    let c3 = (ctx.x2 - cp2.0).hypot(ctx.y2 - cp2.1);
    let pixel_len = c1 + c2 + c3;
    if pixel_len < 1.0 {
        return;
    }
    let steps = ((pixel_len / CURVE_SUBSAMPLE_PX).ceil() as usize).max(1);
    let mut px = ctx.x1;
    let mut py = ctx.y1;
    for step in 1..=steps {
        // t: 归一化进度 [0,1]；u = 1 - t 为互补因子（三次贝塞尔插值）
        let t = step as f32 / steps as f32;
        let u = 1.0 - t;
        let nx = u * u * u * ctx.x1
            + 3.0 * u * u * t * cp1.0
            + 3.0 * u * t * t * cp2.0
            + t * t * t * ctx.x2;
        let ny = u * u * u * ctx.y1
            + 3.0 * u * u * t * cp1.1
            + 3.0 * u * t * t * cp2.1
            + t * t * t * ctx.y2;
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
    for step in 1..=steps {
        // t: 归一化插值进度 [0,1]
        let t = step as f32 * inv;
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
    use lumino_note_core::{AutomationEvent, AutomationTarget};

    fn make_lane(ticks: &[u32], values: &[u16]) -> AutomationLane {
        AutomationLane {
            target: AutomationTarget::CC { controller: 7 },
            track: 0,
            channel: 0,
            events: ticks
                .iter()
                .zip(values.iter())
                .map(|(&tick, &value)| AutomationEvent::new(tick, value, SegmentShape::Step))
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

    #[test]
    fn test_lane_instances_visible_when_anchors_off_viewport() {
        // 回归：锚点（事件）在视口外、但贝塞尔控制柄把曲线延伸到视口内时，
        // 曲线必须仍然渲染（事件窗口需按柄的 tick 偏移扩展）。
        // 场景：事件在 tick 10000（视口外右侧），入向柄向左拉 9000 tick，
        // 曲线延伸到 tick 1000（视口内）。
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
        let mut evt = AutomationEvent::new(10_000, 8192, SegmentShape::Curve { tension: 0 });
        evt.set_in_handle((-9000.0, -3000.0)); // 入向柄向左延伸 9000 tick
        let mut lane = AutomationLane {
            target: AutomationTarget::PitchBend,
            track: 0,
            channel: 0,
            events: vec![evt],
        };
        lane.recompute_auto_handles();
        // 视口 0..200 tick（事件在 10000，远超视口）
        let mut out = Vec::new();
        build_lane_instances(&mut out, 200.0, &view, &lane, [1.0, 1.0, 1.0], false);
        assert!(
            !out.is_empty(),
            "柄延伸进视口的曲线必须渲染（事件窗口已扩展）"
        );
        // 生成的线段应覆盖视口区域（有 x < 200 的实例）
        assert!(
            out.iter().any(|i| i.position[0] < 200.0),
            "曲线应包含视口内的线段"
        );
    }

    #[test]
    fn test_lane_instances_off_viewport_no_handle_still_hidden() {
        // 无柄延伸时：事件全部在视口外 → 只渲染 chase 水平线（保持前一事件值），
        // 不产生任何曲线段（斜线/竖线）
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
        let lane = make_lane(&[10_000, 10_100], &[8192, 9000]);
        let mut out = Vec::new();
        build_lane_instances(&mut out, 200.0, &view, &lane, [1.0, 1.0, 1.0], false);
        assert!(!out.is_empty(), "chase 水平线应存在（保持前一事件值 8192）");
        // 全部为水平线（高度 = 线粗 2px）；无斜线/竖线（高度 > 2）
        assert!(
            out.iter().all(|i| i.size[1] <= 2.0 + 0.01),
            "视口外事件只应有 chase 水平线: {out:?}"
        );
    }
}
