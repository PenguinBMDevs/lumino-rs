//! 滚动条 — yinhe `widgets/scrollbar.rs:1088` 的 iced 迁移
//!
//! 薄 10px，拖拇指/轨道点击，悬浮高亮，边缘拖拽缩放
//! 复用 `lumino-ui-editor/scrollbar_widget` 的三区语义，但厚度 10px，
//! 主题走 `Theme::extended_palette`，与 `lumino_core::ViewState` 联动：
//! - thumb 位置 = scroll / max_scroll
//! - 轨道点击 = 跳转
//! - 中间拖 = 平移 `scroll`
//! - 边缘拖 = 缩放 `zoom`
//! - hover 高亮：idle=w weak, hover=base, dragging=strong

use iced_core::Background;
use iced_core::Border;
use iced_core::Length;
use iced_core::Point;
use iced_core::Rectangle;
use iced_core::Size;
use iced_core::layout;
use iced_core::mouse;
use iced_core::renderer::{self, Quad};
use iced_core::widget::Tree;
use iced_core::{Event, Renderer as _Renderer};

use lumino_core::ViewState;
use lumino_ui_core::{Element, Message, Renderer, Theme};

// ── 常量（薄 10px） ──

/// 横条高度 / 竖条宽度
pub const SCROLLBAR_H: f32 = 10.0;
pub const SCROLLBAR_W: f32 = 10.0;

/// 兼容旧布局的别名（layout.rs 用）
pub const SCROLLBAR_SIZE: f32 = 10.0;

const EDGE_WIDTH_PX: f32 = 4.0;
const THUMB_MIN_SIZE_PX: f32 = 20.0;
const TRACK_GAP: f32 = 1.0;
const THUMB_EDGE_GAP: f32 = 1.0;

// ── 方向 / 边缘 / 状态 ──

/// 滚动条方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

/// 边缘（用于缩放）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Start,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DragZone {
    StartEdge,
    Middle,
    EndEdge,
}

/// 内部状态（Widget::State）
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollbarState {
    Idle,
    Hover,
    HoverEdge(Edge),
    Dragging { start_pos: f32, start_scroll: f32 },
    DraggingEdge {
        start_pos: f32,
        start_zoom: f32,
        start_thumb_size: f32,
        edge: Edge,
    },
}

impl Default for ScrollbarState {
    fn default() -> Self {
        Self::Idle
    }
}

// ── Widget ──

pub struct YinheScrollbar<'a> {
    scroll: f32,
    max_scroll: f32,
    zoom: f32,
    viewport_size: Option<f32>,
    orientation: Orientation,
    on_scroll: Box<dyn Fn(f32) -> Message + 'a>,
    on_zoom: Box<dyn Fn(f32, f32) -> Message + 'a>,
}

impl<'a> YinheScrollbar<'a> {
    pub fn new(
        scroll: f32,
        max_scroll: f32,
        zoom: f32,
        viewport_size: Option<f32>,
        orientation: Orientation,
        on_scroll: impl Fn(f32) -> Message + 'a,
        on_zoom: impl Fn(f32, f32) -> Message + 'a,
    ) -> Self {
        Self {
            scroll,
            max_scroll,
            zoom,
            viewport_size,
            orientation,
            on_scroll: Box::new(on_scroll),
            on_zoom: Box::new(on_zoom),
        }
    }

    pub fn horizontal(
        scroll: f32,
        max_scroll: f32,
        zoom: f32,
        viewport_size: Option<f32>,
        on_scroll: impl Fn(f32) -> Message + 'a,
        on_zoom: impl Fn(f32, f32) -> Message + 'a,
    ) -> Self {
        Self::new(
            scroll,
            max_scroll,
            zoom,
            viewport_size,
            Orientation::Horizontal,
            on_scroll,
            on_zoom,
        )
    }

    pub fn vertical(
        scroll: f32,
        max_scroll: f32,
        zoom: f32,
        viewport_size: Option<f32>,
        on_scroll: impl Fn(f32) -> Message + 'a,
        on_zoom: impl Fn(f32, f32) -> Message + 'a,
    ) -> Self {
        Self::new(
            scroll,
            max_scroll,
            zoom,
            viewport_size,
            Orientation::Vertical,
            on_scroll,
            on_zoom,
        )
    }

    fn effective_viewport(&self, track_size: f32) -> f32 {
        self.viewport_size.unwrap_or(track_size)
    }

    fn actual_max_scroll(&self, track_size: f32) -> f32 {
        let viewport = self.effective_viewport(track_size);
        (self.max_scroll - viewport).max(0.0)
    }

    fn content_fits(&self, track_size: f32) -> bool {
        let viewport = self.effective_viewport(track_size);
        self.max_scroll <= viewport
    }

    fn thumb_geometry(&self, bounds: Rectangle) -> (f32, f32, Rectangle) {
        match self.orientation {
            Orientation::Horizontal => {
                let track_width = (bounds.width - TRACK_GAP * 2.0).max(0.0);
                let effective_viewport = self.effective_viewport(track_width);
                let scrollable_width = (self.max_scroll - effective_viewport).max(0.0);
                let thumb_width = if scrollable_width <= 0.0 {
                    track_width
                } else {
                    (track_width * effective_viewport / self.max_scroll)
                        .max(THUMB_MIN_SIZE_PX)
                        .min(track_width)
                };
                let clamped_scroll = self.scroll.clamp(0.0, scrollable_width);
                let thumb_x = bounds.x
                    + THUMB_EDGE_GAP
                    + if scrollable_width > 0.0 {
                        (clamped_scroll / scrollable_width) * (track_width - thumb_width)
                    } else {
                        0.0
                    };
                let thumb_bounds = Rectangle {
                    x: thumb_x,
                    y: bounds.y + THUMB_EDGE_GAP,
                    width: thumb_width,
                    height: (bounds.height - THUMB_EDGE_GAP * 2.0).max(0.0),
                };
                (track_width, thumb_width, thumb_bounds)
            }
            Orientation::Vertical => {
                let track_height = (bounds.height - TRACK_GAP * 2.0).max(0.0);
                let effective_viewport = self.effective_viewport(track_height);
                let scrollable_height = (self.max_scroll - effective_viewport).max(0.0);
                let thumb_height = if scrollable_height <= 0.0 {
                    track_height
                } else {
                    (track_height * effective_viewport / self.max_scroll)
                        .max(THUMB_MIN_SIZE_PX)
                        .min(track_height)
                };
                let clamped_scroll = self.scroll.clamp(0.0, scrollable_height);
                let thumb_y = bounds.y
                    + THUMB_EDGE_GAP
                    + if scrollable_height > 0.0 {
                        (clamped_scroll / scrollable_height) * (track_height - thumb_height)
                    } else {
                        0.0
                    };
                let thumb_bounds = Rectangle {
                    x: bounds.x + THUMB_EDGE_GAP,
                    y: thumb_y,
                    width: (bounds.width - THUMB_EDGE_GAP * 2.0).max(0.0),
                    height: thumb_height,
                };
                (track_height, thumb_height, thumb_bounds)
            }
        }
    }

    fn get_edge(&self, position: Point, thumb_bounds: Rectangle) -> Option<Edge> {
        match self.orientation {
            Orientation::Horizontal => {
                if position.x >= thumb_bounds.x
                    && position.x <= thumb_bounds.x + EDGE_WIDTH_PX
                {
                    Some(Edge::Start)
                } else if position.x >= thumb_bounds.x + thumb_bounds.width - EDGE_WIDTH_PX
                    && position.x <= thumb_bounds.x + thumb_bounds.width
                {
                    Some(Edge::End)
                } else {
                    None
                }
            }
            Orientation::Vertical => {
                if position.y >= thumb_bounds.y
                    && position.y <= thumb_bounds.y + EDGE_WIDTH_PX
                {
                    Some(Edge::Start)
                } else if position.y >= thumb_bounds.y + thumb_bounds.height - EDGE_WIDTH_PX
                    && position.y <= thumb_bounds.y + thumb_bounds.height
                {
                    Some(Edge::End)
                } else {
                    None
                }
            }
        }
    }

    fn determine_state_at_position(
        &self,
        position: Point,
        thumb_bounds: Rectangle,
    ) -> ScrollbarState {
        if thumb_bounds.contains(position) {
            if let Some(edge) = self.get_edge(position, thumb_bounds) {
                ScrollbarState::HoverEdge(edge)
            } else {
                ScrollbarState::Hover
            }
        } else {
            ScrollbarState::Idle
        }
    }

    fn handle_button_pressed(
        &self,
        state: &mut ScrollbarState,
        geo: &ScrollGeometry,
        cursor: mouse::Cursor,
        shell: &mut iced_core::Shell<'_, Message>,
    ) {
        let Some(position) = cursor.position() else {
            return;
        };
        if geo.thumb_bounds.contains(position) {
            let start_pos = match self.orientation {
                Orientation::Horizontal => position.x,
                Orientation::Vertical => position.y,
            };
            if let Some(edge) = self.get_edge(position, geo.thumb_bounds) {
                *state = ScrollbarState::DraggingEdge {
                    start_pos,
                    start_zoom: self.zoom,
                    start_thumb_size: geo.thumb_size,
                    edge,
                };
            } else {
                let scrollable = self.actual_max_scroll(geo.track_size);
                *state = ScrollbarState::Dragging {
                    start_pos,
                    start_scroll: self.scroll.clamp(0.0, scrollable),
                };
            }
            shell.request_redraw();
        } else if geo.bounds.contains(position) {
            // 轨道点击：跳转
            let relative = match self.orientation {
                Orientation::Horizontal => {
                    (position.x - geo.bounds.x - THUMB_EDGE_GAP)
                        / (geo.track_size - geo.thumb_size).max(1.0)
                }
                Orientation::Vertical => {
                    (position.y - geo.bounds.y - THUMB_EDGE_GAP)
                        / (geo.track_size - geo.thumb_size).max(1.0)
                }
            };
            let actual_max = self.actual_max_scroll(geo.track_size);
            let new_scroll = relative.clamp(0.0, 1.0) * actual_max;
            shell.publish((self.on_scroll)(new_scroll));
            shell.request_redraw();
        }
    }

    fn handle_button_released(
        &self,
        state: &mut ScrollbarState,
        thumb_bounds: Rectangle,
        cursor: mouse::Cursor,
        shell: &mut iced_core::Shell<'_, Message>,
    ) {
        if !matches!(
            state,
            ScrollbarState::Dragging { .. } | ScrollbarState::DraggingEdge { .. }
        ) {
            return;
        }
        let new_state = cursor.position().map_or(ScrollbarState::Idle, |pos| {
            self.determine_state_at_position(pos, thumb_bounds)
        });
        *state = new_state;
        shell.request_redraw();
    }

    fn handle_cursor_moved(
        &self,
        state: &mut ScrollbarState,
        geo: &ScrollGeometry,
        cursor: mouse::Cursor,
        shell: &mut iced_core::Shell<'_, Message>,
    ) {
        match *state {
            ScrollbarState::Dragging {
                start_pos,
                start_scroll,
            } => {
                let Some(pos) = cursor.position() else {
                    return;
                };
                let current = match self.orientation {
                    Orientation::Horizontal => pos.x,
                    Orientation::Vertical => pos.y,
                };
                let delta = current - start_pos;
                let actual_max = self.actual_max_scroll(geo.track_size);
                let ratio = delta / (geo.track_size - geo.thumb_size).max(1.0);
                let new_scroll =
                    (start_scroll + ratio * actual_max).clamp(0.0, actual_max);
                shell.publish((self.on_scroll)(new_scroll));
                shell.request_redraw();
            }
            ScrollbarState::DraggingEdge { .. } => {
                self.handle_dragging_edge(state, geo, cursor, shell);
            }
            _ => {
                if let Some(pos) = cursor.position() {
                    let new_state = self.determine_state_at_position(pos, geo.thumb_bounds);
                    if *state != new_state {
                        *state = new_state;
                        shell.request_redraw();
                    }
                }
            }
        }
    }

    fn handle_dragging_edge(
        &self,
        state: &mut ScrollbarState,
        geo: &ScrollGeometry,
        cursor: mouse::Cursor,
        shell: &mut iced_core::Shell<'_, Message>,
    ) {
        let (start_pos, start_zoom, start_thumb_size, edge) = match *state {
            ScrollbarState::DraggingEdge {
                start_pos,
                start_zoom,
                start_thumb_size,
                edge,
            } => (start_pos, start_zoom, start_thumb_size, edge),
            _ => return,
        };
        let Some(pos) = cursor.position() else {
            return;
        };
        let current = match self.orientation {
            Orientation::Horizontal => pos.x,
            Orientation::Vertical => pos.y,
        };
        let delta = current - start_pos;
        let effective = if edge == Edge::End { delta } else { -delta };
        let fits = self.content_fits(geo.track_size);
        if fits && effective > 0.0 {
            *state = ScrollbarState::DraggingEdge {
                start_pos: current,
                start_zoom: self.zoom,
                start_thumb_size,
                edge,
            };
        } else {
            let max_delta = (geo.track_size - start_thumb_size).max(geo.track_size * 0.5);
            let clamped = effective.clamp(-geo.track_size * 0.9, max_delta);
            let ratio = (1.0 + clamped / start_thumb_size.max(1.0)).max(0.05);
            let new_zoom = (start_zoom / ratio).clamp(0.001, 10.0);
            let fixed = if edge == Edge::End { 0.0 } else { 1.0 };
            shell.publish((self.on_zoom)(new_zoom, fixed));
        }
        shell.request_redraw();
    }
}

struct ScrollGeometry {
    bounds: Rectangle,
    thumb_bounds: Rectangle,
    track_size: f32,
    thumb_size: f32,
}

impl<'a> iced_core::Widget<Message, Theme, Renderer> for YinheScrollbar<'a> {
    fn tag(&self) -> iced_core::widget::tree::Tag {
        iced_core::widget::tree::Tag::of::<ScrollbarState>()
    }

    fn state(&self) -> iced_core::widget::tree::State {
        iced_core::widget::tree::State::new(ScrollbarState::default())
    }

    fn size(&self) -> Size<Length> {
        match self.orientation {
            Orientation::Horizontal => Size {
                width: Length::Fill,
                height: Length::Fixed(SCROLLBAR_H),
            },
            Orientation::Vertical => Size {
                width: Length::Fixed(SCROLLBAR_W),
                height: Length::Fill,
            },
        }
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let size = limits.resolve(self.size().width, self.size().height, Size::new(0.0, 0.0));
        layout::Node::new(size)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: iced_core::Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let palette = theme.extended_palette().background;
        let state = tree.state.downcast_ref::<ScrollbarState>();

        // 背景
        renderer.fill_quad(
            Quad {
                bounds,
                border: Border::default(),
                shadow: iced_core::Shadow::default(),
                snap: false,
            },
            Background::Color(palette.weak.color.scale_alpha(0.35)),
        );

        let (_track, _thumb, thumb_bounds) = self.thumb_geometry(bounds);

        let thumb_color = match state {
            ScrollbarState::Dragging { .. } | ScrollbarState::DraggingEdge { .. } => {
                palette.strong.color
            }
            ScrollbarState::Hover | ScrollbarState::HoverEdge(_) => palette.base.color,
            _ => palette.weak.color,
        };

        renderer.fill_quad(
            Quad {
                bounds: thumb_bounds,
                border: Border {
                    radius: 2.0.into(),
                    width: 0.0,
                    color: iced_core::Color::TRANSPARENT,
                },
                shadow: iced_core::Shadow::default(),
                snap: false,
            },
            Background::Color(thumb_color),
        );

        // 边缘高亮线（悬浮/拖拽时稍亮）
        if matches!(
            state,
            ScrollbarState::HoverEdge(_) | ScrollbarState::DraggingEdge { .. }
        ) {
            if let Some(edge) = match state {
                ScrollbarState::HoverEdge(e) => Some(*e),
                ScrollbarState::DraggingEdge { edge, .. } => Some(*edge),
                _ => None,
            } {
                let line_color = palette.strong.color.scale_alpha(0.9);
                let line_rect = match (self.orientation, edge) {
                    (Orientation::Horizontal, Edge::Start) => Rectangle {
                        x: thumb_bounds.x,
                        y: thumb_bounds.y,
                        width: EDGE_WIDTH_PX.min(thumb_bounds.width),
                        height: thumb_bounds.height,
                    },
                    (Orientation::Horizontal, Edge::End) => Rectangle {
                        x: thumb_bounds.x + thumb_bounds.width - EDGE_WIDTH_PX,
                        y: thumb_bounds.y,
                        width: EDGE_WIDTH_PX.min(thumb_bounds.width),
                        height: thumb_bounds.height,
                    },
                    (Orientation::Vertical, Edge::Start) => Rectangle {
                        x: thumb_bounds.x,
                        y: thumb_bounds.y,
                        width: thumb_bounds.width,
                        height: EDGE_WIDTH_PX.min(thumb_bounds.height),
                    },
                    (Orientation::Vertical, Edge::End) => Rectangle {
                        x: thumb_bounds.x,
                        y: thumb_bounds.y + thumb_bounds.height - EDGE_WIDTH_PX,
                        width: thumb_bounds.width,
                        height: EDGE_WIDTH_PX.min(thumb_bounds.height),
                    },
                };
                renderer.fill_quad(
                    Quad {
                        bounds: line_rect,
                        border: Border::default(),
                        shadow: iced_core::Shadow::default(),
                        snap: false,
                    },
                    Background::Color(line_color),
                );
            }
        }
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: iced_core::Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn iced_core::Clipboard,
        shell: &mut iced_core::Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree.state.downcast_mut::<ScrollbarState>();
        let (track, thumb, thumb_bounds) = self.thumb_geometry(bounds);
        let geo = ScrollGeometry {
            bounds,
            thumb_bounds,
            track_size: track,
            thumb_size: thumb,
        };
        let Event::Mouse(mouse_event) = event else {
            return;
        };
        match mouse_event {
            mouse::Event::ButtonPressed(mouse::Button::Left) => {
                self.handle_button_pressed(state, &geo, cursor, shell);
            }
            mouse::Event::ButtonReleased(mouse::Button::Left) => {
                self.handle_button_released(state, thumb_bounds, cursor, shell);
            }
            mouse::Event::CursorMoved { .. } => {
                self.handle_cursor_moved(state, &geo, cursor, shell);
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        _layout: iced_core::Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<ScrollbarState>();
        match state {
            ScrollbarState::Dragging { .. } => mouse::Interaction::Grabbing,
            ScrollbarState::DraggingEdge { .. } => match self.orientation {
                Orientation::Horizontal => mouse::Interaction::ResizingHorizontally,
                Orientation::Vertical => mouse::Interaction::ResizingVertically,
            },
            ScrollbarState::HoverEdge(_) => match self.orientation {
                Orientation::Horizontal => mouse::Interaction::ResizingHorizontally,
                Orientation::Vertical => mouse::Interaction::ResizingVertically,
            },
            ScrollbarState::Hover => mouse::Interaction::Grab,
            _ => mouse::Interaction::default(),
        }
    }
}

impl<'a> From<YinheScrollbar<'a>> for Element<'a> {
    fn from(w: YinheScrollbar<'a>) -> Self {
        Element::new(w)
    }
}

// ── 兼容旧桩的工厂函数（供 piano_view / arrange 嵌入，同步 ViewState） ──

/// 横向时间轴滚动条（10px 薄）— 关联 `ViewState.scroll_x / zoom_x`
pub fn horizontal<'a>(
    view_width: f32,
    total_ticks: f64,
    scroll_x: f32,
    pixels_per_tick: f32,
    _theme: &'a Theme,
) -> Element<'a> {
    let total_w = (total_ticks as f32 * pixels_per_tick).max(view_width);
    // 同时联动 Piano (ScrollbarScrolled/ZoomXChanged) 与 Arrange (ArrangementScrollX/ZoomX)，
    // 保证 walk pane 与 piano 共享 ViewState 时滚动条与内容滚动同步
    YinheScrollbar::horizontal(
        scroll_x,
        total_w,
        pixels_per_tick,
        Some(view_width),
        |s| Message::Batch(vec![Message::ScrollbarScrolled(s), Message::ArrangementScrollX(s)]),
        |z, r| {
            Message::Batch(vec![
                Message::ZoomXChanged { zoom: z, fixed_ratio: r },
                Message::ArrangementZoomX { zoom: z, fixed_ratio: r },
            ])
        },
    )
    .into()
}

/// 竖向滚动条（值空间）— 自动化面板 / CC
pub fn vertical_value<'a>(
    panel_height: f32,
    total_value: f32,
    value_scroll: f32,
    value_zoom: f32,
    _theme: &'a Theme,
) -> Element<'a> {
    let total = total_value.max(panel_height);
    let _ = panel_height;
    YinheScrollbar::vertical(
        value_scroll,
        total,
        value_zoom,
        Some(panel_height),
        |s| Message::Batch(vec![Message::ScrollbarScrolledY(s), Message::ArrangementScrollY(s)]),
        |z, r| {
            Message::Batch(vec![
                Message::ZoomYChanged { zoom: z, fixed_ratio: r },
                Message::ArrangementZoomY { zoom: z, fixed_ratio: r },
            ])
        },
    )
    .into()
}

/// 像素空间竖向滚动条（track / key 轴）
pub fn vertical_pixel<'a>(
    view_height: f32,
    num_cells: usize,
    cell_size: f32,
    scroll: f32,
    _theme: &'a Theme,
) -> Element<'a> {
    let total = (num_cells as f32 * cell_size).max(view_height);
    YinheScrollbar::vertical(
        scroll,
        total,
        cell_size,
        Some(view_height),
        |s| Message::Batch(vec![Message::ScrollbarScrolledY(s), Message::ArrangementScrollY(s)]),
        |z, r| {
            Message::Batch(vec![
                Message::ZoomYChanged { zoom: z, fixed_ratio: r },
                Message::ArrangementZoomY { zoom: z, fixed_ratio: r },
            ])
        },
    )
    .into()
}

/// 带视口的通用水平滚动条（供外部直接传 max/scroll/zoom）
pub fn horizontal_with_viewport<'a>(
    scroll_x: f32,
    max_scroll: f32,
    zoom_x: f32,
    viewport_w: f32,
) -> Element<'a> {
    YinheScrollbar::horizontal(
        scroll_x,
        max_scroll,
        zoom_x,
        Some(viewport_w),
        |s| Message::ScrollbarScrolled(s),
        |z, r| Message::ZoomXChanged { zoom: z, fixed_ratio: r },
    )
    .into()
}

pub fn vertical_with_viewport<'a>(
    scroll_y: f32,
    max_scroll: f32,
    zoom_y: f32,
    viewport_h: f32,
) -> Element<'a> {
    YinheScrollbar::vertical(
        scroll_y,
        max_scroll,
        zoom_y,
        Some(viewport_h),
        |s| Message::ScrollbarScrolledY(s),
        |z, r| Message::ZoomYChanged { zoom: z, fixed_ratio: r },
    )
    .into()
}

/// 便捷：直接从 `ViewState` 构造横向滚动条（联动 scroll_x / zoom_x）
pub fn horizontal_for_view<'a>(view: &ViewState, viewport_w: f32, theme: &'a Theme) -> Element<'a> {
    horizontal(
        viewport_w,
        view.total_ticks as f64,
        view.scroll_x,
        view.zoom_x,
        theme,
    )
}

/// 便捷：直接从 `ViewState` 构造纵向像素滚动条（key 轴，联动 scroll_y / zoom_y）
pub fn vertical_pixel_for_view<'a>(
    view: &ViewState,
    viewport_h: f32,
    theme: &'a Theme,
) -> Element<'a> {
    vertical_pixel(
        viewport_h,
        view.visible_key_count as usize,
        view.zoom_y,
        view.scroll_y,
        theme,
    )
}
