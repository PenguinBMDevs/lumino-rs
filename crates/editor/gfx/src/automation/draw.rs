//! 单条线段渲染（Step / Curve / 贝塞尔折线）

use crate::cc_bar_renderer::CcBarInstance;
use lumino_note_core::automation::SegmentShape;

/// 曲线子采样像素步长。Linear/Curve 段按此步长采样并连成多条短线。
const CURVE_SUBSAMPLE_PX: f32 = 2.0;
/// 线段不透明度。
const LINE_ALPHA: f32 = 0.85;

/// 单条线段渲染上下文。
pub(super) struct SegmentContext {
    /// 起点 X 坐标
    pub(super) x1: f32,
    /// 起点 Y 坐标
    pub(super) y1: f32,
    /// 终点 X 坐标
    pub(super) x2: f32,
    /// 终点 Y 坐标
    pub(super) y2: f32,
    /// 线段形状
    pub(super) shape: SegmentShape,
    /// 贝塞尔控制柄（屏幕坐标）
    pub(super) cp1: Option<(f32, f32)>,
    pub(super) cp2: Option<(f32, f32)>,
    /// 线条颜色
    pub(super) color: [f32; 3],
    /// 线条粗细
    pub(super) thickness: f32,
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

/// 渲染单条线段（Step 或 Curve）。
pub(super) fn render_segment(out: &mut Vec<CcBarInstance>, ctx: &SegmentContext) {
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
