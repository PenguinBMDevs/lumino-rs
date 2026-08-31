//! 自动化面板布局 — 对应 yinhe `automation_panel/layout.rs`
//!
//! 不自建 wgpu：`sync_renderer_count` 的 `InstanceRenderer/RenderContext` 同步
//! 已移除，改为 iced `Rectangle` 几何与滚动状态计算；渲染资源由
//! `lumino-gfx::{CcBarRenderer, Context, SwappableBuffer}` 侧持有。

use iced_core::{Point, Rectangle, Size};

use super::constants::SPLIT_H;
use super::types::{AutomationPanelView, PanelsLayout};

/// 帧上下文（滚动/裁剪/可见区），供每面板循环使用。
#[derive(Clone, Copy, Debug)]
pub struct FrameCtx {
    pub orig_heights: [f32; 8],
    pub orig_len: usize,
    pub max_scroll: f32,
    pub scroll_y: f32,
    pub panels_area_rect: Rectangle,
    pub vbar_rect: Rectangle,
    pub y_offset: f32,
    pub visible_top: f32,
    pub visible_bottom: f32,
}

/// 将 `ViewState` 的滚动/缩放同步到各面板的 `base`，并计算可见区几何。
///
/// 对齐 yinhe `layout::begin_frame` 的滚动/裁剪语义，但以 iced `Rectangle`
/// 与 `AutomationPanelState::scroll_y`（`Program::State`）替代 egui 的
/// `ui.data().get_persisted`。
#[must_use]
pub fn begin_frame(
    panels: &[AutomationPanelView],
    layout: PanelsLayout,
    incoming_scroll_y: f32,
) -> FrameCtx {
    let n = panels.len().min(8);
    let mut orig_heights = [0.0; 8];
    for (i, p) in panels.iter().take(8).enumerate() {
        orig_heights[i] = p.panel_height;
    }
    let panels_natural_h: f32 =
        panels.iter().map(|p| p.panel_height).sum::<f32>() + (panels.len() as f32 * SPLIT_H);
    let max_scroll = (panels_natural_h - layout.panels_visible_h).max(0.0);
    let scroll_y = incoming_scroll_y.clamp(0.0, max_scroll);

    let panels_area_rect = layout.content_rect;
    let vbar_w = 8.0;
    let vbar_rect = Rectangle::new(
        Point::new(
            panels_area_rect.x + panels_area_rect.width - vbar_w,
            panels_area_rect.y,
        ),
        Size::new(vbar_w, panels_area_rect.height),
    );

    let y_offset = panels_area_rect.y - scroll_y;
    let visible_top = panels_area_rect.y;
    let visible_bottom = panels_area_rect.y + panels_area_rect.height;

    FrameCtx {
        orig_heights,
        orig_len: n,
        max_scroll,
        scroll_y,
        panels_area_rect,
        vbar_rect,
        y_offset,
        visible_top,
        visible_bottom,
    }
}

/// 计算单个面板在 `FrameCtx` 中的布局矩形。
///
/// 返回 `(panel_rect, grid_area, combo_area, handle_rect)`，均在 iced 画布本地坐标。
#[must_use]
pub fn panel_rects(
    ctx: &FrameCtx,
    layout: PanelsLayout,
    panel: &AutomationPanelView,
    index: usize,
) -> Option<(Rectangle, Rectangle, Rectangle, Rectangle)> {
    let mut y = ctx.y_offset;
    for i in 0..index {
        y += ctx.orig_heights[i] + SPLIT_H;
    }
    let panel_y = y;
    let panel_h = panel.panel_height;
    // 视口裁剪：完全不可见时跳过
    if panel_y + panel_h < ctx.visible_top || panel_y > ctx.visible_bottom {
        return None;
    }
    let panel_rect = Rectangle::new(
        Point::new(ctx.panels_area_rect.x, panel_y),
        Size::new(ctx.panels_area_rect.width, panel_h),
    );
    let combo_rect = Rectangle::new(
        Point::new(panel_rect.x, panel_rect.y),
        Size::new(layout.combo_width, panel_rect.height),
    );
    let grid_area = Rectangle::new(
        Point::new(panel_rect.x + layout.combo_width, panel_rect.y),
        Size::new(
            (panel_rect.width - layout.combo_width).max(0.0),
            panel_rect.height,
        ),
    );
    let handle_rect = Rectangle::new(
        Point::new(panel_rect.x, panel_rect.y + panel_rect.height),
        Size::new(panel_rect.width, SPLIT_H),
    );
    Some((panel_rect, grid_area, combo_rect, handle_rect))
}

/// 将 `AutomationPanelView` 的水平状态批量同步为 pianoroll 的 `scroll_x / ppt`。
pub fn sync_panels_from_pianoroll(
    panels: &mut [AutomationPanelView],
    scroll_x: f32,
    pixels_per_tick: f32,
    left_panel_width: f32,
) {
    for p in panels {
        p.sync_from_view_state(scroll_x, pixels_per_tick, left_panel_width);
    }
}

/// 以 `content_rect` 与可见高度构造 `PanelsLayout`。
#[must_use]
pub fn make_panels_layout(content_rect: Rectangle, combo_width: f32) -> PanelsLayout {
    PanelsLayout {
        combo_width,
        content_rect,
        panels_visible_h: content_rect.height,
    }
}
