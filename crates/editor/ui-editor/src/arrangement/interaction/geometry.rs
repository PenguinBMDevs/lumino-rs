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
///
/// 支持拍号变化：吸附间隔在拍号段内按段内拍长计算，且从段起点对齐，
/// 保证框选/移动结果与按拍号绘制的网格线一致（避免"框选框错位"）。
/// `time_signatures` 为空时回退到旧行为（固定 ppq 间隔）。
pub fn snap_tick(
    tick: f64,
    precision: NotePrecision,
    ppq: u16,
    time_signatures: &[(u32, u8, u8)],
) -> f64 {
    if time_signatures.is_empty() {
        let interval = precision.as_ticks(ppq) as f64;
        if interval <= 0.0 {
            return tick;
        }
        return (tick / interval).round() * interval;
    }

    let (seg_start, numerator, denominator) = ts_segment_at(tick, time_signatures);
    let beat = ppq as f64 * 4.0 / denominator.max(1) as f64;
    let measure = beat * numerator.max(1) as f64;
    // 全音符（小节）精度吸附到段内小节边界；其余精度按拍长的倍数（与
    // NotePrecision 语义一致：Quarter=1 拍、Half=2 拍、Eighth=1/2 拍…）
    let interval = if precision == NotePrecision::Whole {
        measure
    } else {
        beat * (precision.as_ticks(ppq) / ppq as f32) as f64
    };
    if interval <= 0.0 {
        return tick;
    }
    let offset = tick - seg_start as f64;
    seg_start as f64 + (offset / interval).round() * interval
}

/// 返回 tick 所在拍号段 (段起点 tick, 分子, 分母)；空列表回退 4/4（起点 0）。
fn ts_segment_at(tick: f64, time_signatures: &[(u32, u8, u8)]) -> (u32, u8, u8) {
    let mut active = (0_u32, 4_u8, 4_u8);
    for &(ts_tick, num, den) in time_signatures {
        if tick >= ts_tick as f64 {
            active = (ts_tick, num, den);
        } else {
            break;
        }
    }
    active
}

/// 计算框选/橡皮擦的对齐边界。
/// 返回 `(view_sx, view_ex, view_sy, view_ey, t_start, t_end, track_lo, track_hi)`。
#[allow(clippy::too_many_arguments)]
pub fn arrange_snapped_bounds(
    start: Point,
    end: Point,
    viewport: &ArrangementViewport,
    precision: NotePrecision,
    ppq: u16,
    time_signatures: &[(u32, u8, u8)],
) -> (f32, f32, f32, f32, f64, f64, usize, usize) {
    let sx = start.x.min(end.x);
    let ex = start.x.max(end.x);
    let sy = start.y.min(end.y);
    let ey = start.y.max(end.y);

    let tick_s = viewport.x_to_tick(sx + viewport.scroll_x);
    let tick_e = viewport.x_to_tick(ex + viewport.scroll_x);
    let snapped_s = snap_tick(tick_s, precision, ppq, time_signatures);
    let snapped_e = snap_tick(tick_e, precision, ppq, time_signatures);
    let t_start = snapped_s.min(snapped_e);
    let mut t_end = snapped_s.max(snapped_e);

    let interval = snap_interval(precision, ppq, time_signatures);
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

/// 计算当前拍号段下的吸附间隔（snap_tick 内部逻辑的复刻，用于最小宽度兜底）
fn snap_interval(precision: NotePrecision, ppq: u16, time_signatures: &[(u32, u8, u8)]) -> f64 {
    if time_signatures.is_empty() {
        return precision.as_ticks(ppq) as f64;
    }
    // 以 tick 0 所在段为准即可（框选起点一般接近 0；段变化时最小宽度略保守无害）
    let (_, numerator, denominator) = ts_segment_at(0.0, time_signatures);
    let beat = ppq as f64 * 4.0 / denominator.max(1) as f64;
    if precision == NotePrecision::Whole {
        beat * numerator.max(1) as f64
    } else {
        beat * (precision.as_ticks(ppq) / ppq as f32) as f64
    }
}
