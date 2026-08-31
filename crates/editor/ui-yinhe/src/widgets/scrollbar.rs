//! 滚动条 — yinhe `widgets/scrollbar.rs:1088` 的 iced 迁移桩
//!
//! 原 `egui` 实现分三区（左边缘缩放/中间平移/右边缘缩放）并以 `interact` + `drag_delta`
//! 驱动 `scroll_x / pixels_per_tick` 等；iced 桩以 `canvas::Program` 重建，
//! 保持同款三区语义（背景不触发、thumb 内才响应），主题取 `Theme::extended_palette`。

use iced_core::mouse::{self, Cursor};
use iced_core::{Color, Length, Point, Rectangle, Size, Vector};
use iced_widget::canvas::{self, Cache, Frame, Geometry, Path, Program, Stroke};

use lumino_ui_core::{Renderer, Theme};

const EDGE_WIDTH: f32 = 4.0;

/// 滚动条方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

/// 滚动条状态（由 `Program::State` 持久化，替代 yinhe `egui::Id`）
#[derive(Debug, Default)]
pub struct ScrollbarState {
    pub is_dragging: Option<DragZone>,
    pub drag_start: Option<Point>,
    cache: Cache<Renderer>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragZone {
    StartEdge,
    Middle,
    EndEdge,
}

/// 水平滚动条（时间轴）Canvas Program
///
/// - `view_width`：视口像素宽
/// - `total_ticks`：总 tick 数
/// - `scroll_x / pixels_per_tick`：双向绑定（Host 持有，Program 仅在 `on_event` 中回写 `Message` 占位）
pub struct HorizontalScrollbar<'a> {
    pub view_width: f32,
    pub total_ticks: f64,
    pub scroll_x: f32,
    pub pixels_per_tick: f32,
    pub theme: &'a Theme,
}

impl<'a> Program<lumino_ui_core::Message, Theme, Renderer> for HorizontalScrollbar<'a> {
    type State = ScrollbarState;

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry<Renderer>> {
        let mut frame = Frame::new(renderer, bounds.size());
        let palette = theme.extended_palette();
        let bg = palette.background.weak.color.scale_alpha(0.5);
        let thumb = palette.background.strong.color;

        // 背景
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), bg);

        if self.total_ticks <= 0.0 || bounds.width <= 0.0 || self.pixels_per_tick <= 0.0 {
            return vec![frame.into_geometry()];
        }

        let max_scroll =
            (self.total_ticks as f32 * self.pixels_per_tick - self.view_width).max(0.0);
        let scroll = self.scroll_x.clamp(0.0, max_scroll);
        let scale = bounds.width as f64 / self.total_ticks;
        let start_tick = scroll as f64 / self.pixels_per_tick as f64;
        let viewport_ticks = self.view_width as f64 / self.pixels_per_tick as f64;
        let left = (start_tick * scale) as f32;
        let width = (viewport_ticks * scale) as f32;

        // Thumb
        let thumb_rect = Rectangle::new(
            Point::new(left, 0.0),
            Size::new(width.min(bounds.width - left), bounds.height),
        );
        frame.fill_rectangle(thumb_rect.position(), thumb_rect.size(), thumb);

        // 边缘线
        let edge_color = palette.background.base.text.scale_alpha(0.6);
        let left_edge = Rectangle::new(
            Point::new(left, 0.0),
            Size::new(EDGE_WIDTH.min(width), bounds.height),
        );
        let right_edge = Rectangle::new(
            Point::new((left + width - EDGE_WIDTH).max(left), 0.0),
            Size::new(EDGE_WIDTH.min(width), bounds.height),
        );
        frame.stroke(
            &Path::rectangle(left_edge.position(), left_edge.size()),
            Stroke::default().with_color(edge_color).with_width(1.0),
        );
        frame.stroke(
            &Path::rectangle(right_edge.position(), right_edge.size()),
            Stroke::default().with_color(edge_color).with_width(1.0),
        );

        vec![frame.into_geometry()]
    }

    fn update(
        &self,
        state: &mut Self::State,
        event: &iced_core::Event,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> Option<canvas::Action<lumino_ui_core::Message>> {
        let pos = cursor.position()?;
        if !bounds.contains(pos) {
            return None;
        }
        let rel = Point::new(pos.x - bounds.x, pos.y - bounds.y);
        match event {
            iced_core::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                // 判定三区（简化：左 4px / 右 4px / 中间）
                let max_scroll =
                    (self.total_ticks as f32 * self.pixels_per_tick - self.view_width).max(0.0);
                let scroll = self.scroll_x.clamp(0.0, max_scroll);
                let scale = bounds.width as f64 / self.total_ticks;
                let start_tick = scroll as f64 / self.pixels_per_tick as f64;
                let viewport_ticks = self.view_width as f64 / self.pixels_per_tick as f64;
                let left = (start_tick * scale) as f32;
                let width = (viewport_ticks * scale) as f32;
                let right = left + width;
                let zone = if rel.x >= left && rel.x < left + EDGE_WIDTH {
                    DragZone::StartEdge
                } else if rel.x > right - EDGE_WIDTH && rel.x <= right {
                    DragZone::EndEdge
                } else if rel.x >= left && rel.x <= right {
                    DragZone::Middle
                } else {
                    return None;
                };
                state.is_dragging = Some(zone);
                state.drag_start = Some(rel);
                state.cache.clear();
                None
            }
            iced_core::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.is_dragging = None;
                state.drag_start = None;
                state.cache.clear();
                None
            }
            iced_core::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if state.is_dragging.is_some() {
                    state.cache.clear();
                    // 实际平移/缩放由 Host 通过 Message 回写 scroll_x / ppt，
                    // 此处仅触发重绘（stub），后续由 Host 接入 `zoom_factor_from_delta` 等
                    return Some(canvas::Action::request_redraw());
                }
                None
            }
            _ => None,
        }
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> mouse::Interaction {
        if state.is_dragging == Some(DragZone::Middle) {
            return mouse::Interaction::Grabbing;
        }
        let Some(pos) = cursor.position() else {
            return mouse::Interaction::None;
        };
        if !bounds.contains(pos) {
            return mouse::Interaction::None;
        }
        let rel = Point::new(pos.x - bounds.x, pos.y - bounds.y);
        let max_scroll =
            (self.total_ticks as f32 * self.pixels_per_tick - self.view_width).max(0.0);
        let scroll = self.scroll_x.clamp(0.0, max_scroll);
        let scale = bounds.width as f64 / self.total_ticks;
        let start_tick = scroll as f64 / self.pixels_per_tick as f64;
        let viewport_ticks = self.view_width as f64 / self.pixels_per_tick as f64;
        let left = (start_tick * scale) as f32;
        let width = (viewport_ticks * scale) as f32;
        let right = left + width;
        if (rel.x >= left && rel.x < left + EDGE_WIDTH)
            || (rel.x > right - EDGE_WIDTH && rel.x <= right)
        {
            mouse::Interaction::ResizingHorizontally
        } else if rel.x >= left && rel.x <= right {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::None
        }
    }
}

/// 构建水平滚动条 Element（供 `piano_view` / `arrange` 嵌入）
///
/// 高度固定 `SCROLLBAR_H`（对齐 yinhe `theme::SCROLLBAR_H`），
/// 背景与 thumb 配色走 `Theme`，交互走 `Program::State`。
pub fn horizontal<'a>(
    view_width: f32,
    total_ticks: f64,
    scroll_x: f32,
    pixels_per_tick: f32,
    theme: &'a Theme,
) -> iced_core::Element<'a, lumino_ui_core::Message, Theme, Renderer> {
    use iced_widget::canvas::Canvas;
    Canvas::new(HorizontalScrollbar {
        view_width,
        total_ticks,
        scroll_x,
        pixels_per_tick,
        theme,
    })
    .width(Length::Fill)
    .height(Length::Fixed(16.0))
    .into()
}

/// 垂直滚动条（值空间）Canvas Program — 用于自动化面板/钢琴卷帘 key 轴
pub struct VerticalScrollbar<'a> {
    pub panel_height: f32,
    pub total_value: f32,
    pub value_scroll: f32,
    pub value_zoom: f32,
    pub theme: &'a Theme,
}

impl<'a> Program<lumino_ui_core::Message, Theme, Renderer> for VerticalScrollbar<'a> {
    type State = ScrollbarState;

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry<Renderer>> {
        let mut frame = Frame::new(renderer, bounds.size());
        let palette = theme.extended_palette();
        let bg = palette.background.weak.color.scale_alpha(0.5);
        let thumb = palette.background.strong.color;
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), bg);

        if bounds.height <= 0.0 || self.total_value <= 0.0 {
            return vec![frame.into_geometry()];
        }
        let visible = self.total_value / self.value_zoom.max(0.01);
        let scale = bounds.height / self.total_value.max(visible);
        let top_val = self.value_scroll + visible;
        let bottom_val = self.value_scroll;
        let rect_top = ((self.total_value - top_val) * scale).max(0.0);
        let rect_bottom = ((self.total_value - bottom_val) * scale).min(bounds.height);
        let rect_h = (rect_bottom - rect_top).max(4.0);
        let thumb_rect = Rectangle::new(Point::new(0.0, rect_top), Size::new(bounds.width, rect_h));
        frame.fill_rectangle(thumb_rect.position(), thumb_rect.size(), thumb);
        vec![frame.into_geometry()]
    }
}

pub fn vertical_value<'a>(
    panel_height: f32,
    total_value: f32,
    value_scroll: f32,
    value_zoom: f32,
    theme: &'a Theme,
) -> iced_core::Element<'a, lumino_ui_core::Message, Theme, Renderer> {
    use iced_widget::canvas::Canvas;
    Canvas::new(VerticalScrollbar {
        panel_height,
        total_value,
        value_scroll,
        value_zoom,
        theme,
    })
    .width(Length::Fixed(16.0))
    .height(Length::Fill)
    .into()
}

/// 像素空间垂直滚动条（track 轴 / key 轴）
pub struct VerticalPixelScrollbar<'a> {
    pub view_height: f32,
    pub num_cells: usize,
    pub cell_size: f32,
    pub scroll: f32,
    pub theme: &'a Theme,
}

impl<'a> Program<lumino_ui_core::Message, Theme, Renderer> for VerticalPixelScrollbar<'a> {
    type State = ScrollbarState;
    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry<Renderer>> {
        let mut frame = Frame::new(renderer, bounds.size());
        let palette = theme.extended_palette();
        let bg = palette.background.weak.color.scale_alpha(0.5);
        let thumb = palette.background.strong.color;
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), bg);
        if bounds.height <= 0.0 || self.num_cells == 0 {
            return vec![frame.into_geometry()];
        }
        let total = self.num_cells as f32 * self.cell_size;
        let max_scroll = (total - self.view_height).max(0.0);
        let scroll = self.scroll.clamp(0.0, max_scroll);
        let scale = bounds.height / total.max(self.view_height);
        let top = (scroll * scale).clamp(0.0, bounds.height);
        let h = (self.view_height * scale).min(bounds.height - top);
        let thumb_rect = Rectangle::new(Point::new(0.0, top), Size::new(bounds.width, h.max(4.0)));
        frame.fill_rectangle(thumb_rect.position(), thumb_rect.size(), thumb);
        vec![frame.into_geometry()]
    }
}

pub fn vertical_pixel<'a>(
    view_height: f32,
    num_cells: usize,
    cell_size: f32,
    scroll: f32,
    theme: &'a Theme,
) -> iced_core::Element<'a, lumino_ui_core::Message, Theme, Renderer> {
    use iced_widget::canvas::Canvas;
    Canvas::new(VerticalPixelScrollbar {
        view_height,
        num_cells,
        cell_size,
        scroll,
        theme,
    })
    .width(Length::Fixed(16.0))
    .height(Length::Fill)
    .into()
}
