//! 框选（Marquee）拖动 — 对应 `yinhe piano_view/marquee.rs:498`
//!
//! 提供 `marquee_drag_frame` 的 iced 侧生命周期：
//! - `Press → Move（auto-scroll） → Release` 三段式
//! - 距离 ≥ 3px 才产生有效选区，否则视为光标点击
//! - 状态由 `PianoDragState::marquee_*` 管理，不再使用 `egui::Id::persisted`

use iced_core::{Point, Rectangle};

use lumino_core::ViewState;

use crate::piano_view::drag::state::PianoDragState;

/// 框选拖动结果（对应 yinhe `MarqueeDragResult`）
#[derive(Debug, Clone)]
pub struct MarqueeResult {
    /// 起始 tick（含 snap）
    pub t_start: f64,
    /// 结束 tick（含 snap，需 ≥ interval）
    pub t_end: f64,
    /// 最低 key
    pub key_lo: u8,
    /// 最高 key
    pub key_hi: u8,
    /// 本地像素矩形（已 snap 到量化网格与整键单元）
    pub snapped_rect: Rectangle,
}

/// 开始框选（按下时调用）
pub fn marquee_press(state: &mut PianoDragState, local_pos: Point) {
    state.start_marquee(local_pos);
}

/// 拖动中更新（移动时调用，含 auto-scroll 占位）
///
/// yinhe `auto_scroll_on_drag_dir` 在此处驱动 `ViewState::scroll_*`；
/// iced 桩保留接口，P3 接入真实 scroll 同步。
pub fn marquee_move(
    state: &mut PianoDragState,
    _view: &mut ViewState,
    local_pos: Point,
    _bounds: Rectangle,
) {
    state.update_marquee(local_pos);
    // TODO(P3): auto-scroll：指针贴近边界时滚动 view，复用 `Viewport::set_scroll_x/y`
}

/// 释放并计算 snapped 选区（松手时调用）
///
/// `snap_tick` 为上层 `ViewState::snap_tick` 封装（含拍号感知的 `snap_tick_ceil/floor`）
/// P3 接入真实量化间隔后，此处按 `quantize.tick_interval(ppq)` 保证最小宽度 1 grid。
pub fn marquee_release(
    state: &mut PianoDragState,
    view: &ViewState,
    snap_tick: impl Fn(f32) -> f32,
) -> Option<MarqueeResult> {
    let rect = state.marquee_rect()?;
    let (x0, x1) = (
        rect.x.min(rect.x + rect.width),
        rect.x.max(rect.x + rect.width),
    );
    let (y0, y1) = (
        rect.y.min(rect.y + rect.height),
        rect.y.max(rect.y + rect.height),
    );

    let tick_s = snap_tick(view.x_to_tick(x0));
    let tick_e = snap_tick(view.x_to_tick(x1));
    let t_start = tick_s.min(tick_e) as f64;
    let mut t_end = tick_s.max(tick_e) as f64;
    if t_end <= t_start {
        t_end = t_start + view.snap_precision as f64;
    }
    let key_lo = view.y_to_key(y1).min(127) as u8;
    let key_hi = view.y_to_key(y0).min(127) as u8;
    let (lo, hi) = (key_lo.min(key_hi), key_lo.max(key_hi));

    // snapped_rect：主轴对齐量化后的像素，副轴对齐整键单元（与 yinhe `piano_snapped_bounds` 一致）
    let snap_x0 = view.tick_to_x(t_start as f32);
    let snap_x1 = view.tick_to_x(t_end as f32);
    let snap_y0 = view.key_to_y(hi as u16);
    let snap_y1 = view.key_to_y(lo as u16) + view.zoom_y;
    let snapped = Rectangle::new(
        Point::new(snap_x0.min(snap_x1), snap_y0.min(snap_y1)),
        iced_core::Size::new((snap_x0 - snap_x1).abs(), (snap_y0 - snap_y1).abs()),
    );

    state.clear();
    Some(MarqueeResult {
        t_start,
        t_end,
        key_lo: lo,
        key_hi: hi,
        snapped_rect: snapped,
    })
}
