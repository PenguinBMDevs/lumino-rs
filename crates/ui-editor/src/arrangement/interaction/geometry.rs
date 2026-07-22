//! 工程走带交互几何工具函数

use iced_core::{Point, Rectangle};

use lumino_core::NotePrecision;

use crate::arrangement::ArrangementViewport;

/// 将屏幕坐标转换为 Canvas 局部坐标。
#[inline]
pub fn local_pos(pos: Point, bounds: Rectangle) -> Point {
    Point::new(pos.x - bounds.x, pos.y - bounds.y)
}

/// 将屏幕坐标限制在 Canvas 边界内后转换为局部坐标。
#[inline]
pub fn clamped_local(pos: Point, bounds: Rectangle) -> Point {
    Point::new(
        pos.x.clamp(bounds.x, bounds.x + bounds.width) - bounds.x,
        pos.y.clamp(bounds.y, bounds.y + bounds.height) - bounds.y,
    )
}

/// 判断局部坐标是否落在给定选择矩形内。
pub fn inside_selection_rect(
    local: Point,
    arr_sel_rect: Option<(f64, f64, usize, usize)>,
    viewport: &ArrangementViewport,
) -> bool {
    let Some((t_start, t_end, track_lo, track_hi)) = arr_sel_rect else {
        return false;
    };
    let lh = viewport.lane_height();
    let scroll_y = viewport.scroll_y;
    let scroll_x = viewport.scroll_x;
    let sy = track_lo as f32 * lh - scroll_y;
    let ey = (track_hi as f32 + 1.0) * lh - scroll_y;
    let sx = viewport.tick_to_x(t_start) - scroll_x;
    let ex = viewport.tick_to_x(t_end) - scroll_x;
    let min_x = sx.min(ex);
    let max_x = sx.max(ex);
    let min_y = sy.min(ey);
    let max_y = sy.max(ey);
    local.x >= min_x && local.x <= max_x && local.y >= min_y && local.y <= max_y
}

/// 将 tick 对齐到当前精度网格。
pub fn snap_tick(tick: f64, precision: NotePrecision, ppq: u16) -> f64 {
    let interval = precision.as_ticks(ppq) as f64;
    if interval <= 0.0 {
        return tick;
    }
    (tick / interval).round() * interval
}

/// 计算框选/橡皮擦的对齐边界。
/// 返回 `(view_sx, view_ex, view_sy, view_ey, t_start, t_end, track_lo, track_hi)`。
pub fn arrange_snapped_bounds(
    start: Point,
    end: Point,
    viewport: &ArrangementViewport,
    precision: NotePrecision,
    ppq: u16,
) -> (f32, f32, f32, f32, f64, f64, usize, usize) {
    let sx = start.x.min(end.x);
    let ex = start.x.max(end.x);
    let sy = start.y.min(end.y);
    let ey = start.y.max(end.y);

    let tick_s = viewport.x_to_tick(sx + viewport.scroll_x);
    let tick_e = viewport.x_to_tick(ex + viewport.scroll_x);
    let snapped_s = snap_tick(tick_s, precision, ppq);
    let snapped_e = snap_tick(tick_e, precision, ppq);
    let t_start = snapped_s.min(snapped_e);
    let mut t_end = snapped_s.max(snapped_e);

    let interval = precision.as_ticks(ppq) as f64;
    if t_end <= t_start {
        t_end = t_start + interval.max(1.0);
    }

    let lh = viewport.lane_height();
    let scroll_y = viewport.scroll_y;
    let track_lo = ((scroll_y + sy) / lh).floor().max(0.0) as usize;
    let track_hi = ((scroll_y + ey) / lh).floor().max(0.0) as usize;

    let view_sy = track_lo as f32 * lh - scroll_y;
    let view_ey = (track_hi as f32 + 1.0) * lh - scroll_y;

    let view_sx = viewport.tick_to_x(t_start);
    let view_ex = viewport.tick_to_x(t_end);

    (
        view_sx, view_ex, view_sy, view_ey, t_start, t_end, track_lo, track_hi,
    )
}
