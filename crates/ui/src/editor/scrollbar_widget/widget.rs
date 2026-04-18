use super::types::{Edge, ScrollbarOrientation, ScrollbarState};
use crate::Message;
use crate::constants::scrollbar as scrollbar_constants;
use iced_core::Rectangle;

pub struct ScrollbarWidget<'a> {
    pub scroll: f32,
    pub max_scroll: f32,
    pub zoom: f32,
    pub orientation: ScrollbarOrientation,
    pub on_scroll: Box<dyn Fn(f32) -> Message + 'a>,
    pub on_zoom: Box<dyn Fn(f32, f32) -> Message + 'a>,
}

impl<'a> ScrollbarWidget<'a> {
    pub fn new(
        scroll: f32,
        max_scroll: f32,
        zoom: f32,
        orientation: ScrollbarOrientation,
        on_scroll: impl Fn(f32) -> Message + 'a,
        on_zoom: impl Fn(f32, f32) -> Message + 'a,
    ) -> Self {
        Self {
            scroll,
            max_scroll,
            zoom,
            orientation,
            on_scroll: Box::new(on_scroll),
            on_zoom: Box::new(on_zoom),
        }
    }

    pub fn horizontal(
        scroll_x: f32,
        max_scroll: f32,
        zoom_x: f32,
        on_scroll: impl Fn(f32) -> Message + 'a,
        on_zoom: impl Fn(f32, f32) -> Message + 'a,
    ) -> Self {
        Self::new(
            scroll_x,
            max_scroll,
            zoom_x,
            ScrollbarOrientation::Horizontal,
            on_scroll,
            on_zoom,
        )
    }

    pub fn vertical(
        scroll_y: f32,
        max_scroll: f32,
        zoom_y: f32,
        on_scroll: impl Fn(f32) -> Message + 'a,
        on_zoom: impl Fn(f32, f32) -> Message + 'a,
    ) -> Self {
        Self::new(
            scroll_y,
            max_scroll,
            zoom_y,
            ScrollbarOrientation::Vertical,
            on_scroll,
            on_zoom,
        )
    }

    pub(crate) fn thumb_geometry(&self, bounds: Rectangle) -> (f32, f32, Rectangle) {
        match self.orientation {
            ScrollbarOrientation::Horizontal => {
                let track_width = bounds.width - scrollbar_constants::TRACK_THUMB_GAP * 2.0;
                let scrollable_width = (self.max_scroll - track_width).max(0.0);

                let thumb_width = if scrollable_width <= 0.0 {
                    track_width
                } else {
                    (track_width * track_width / self.max_scroll)
                        .max(scrollbar_constants::THUMB_MIN_SIZE_PX)
                        .min(track_width)
                };

                let clamped_scroll = self.scroll.clamp(0.0, scrollable_width);
                let thumb_x = bounds.x
                    + scrollbar_constants::THUMB_TRACK_EDGE_GAP
                    + (clamped_scroll / scrollable_width.max(1.0)) * (track_width - thumb_width);

                let thumb_bounds = Rectangle {
                    x: thumb_x,
                    y: bounds.y + scrollbar_constants::THUMB_TRACK_EDGE_GAP,
                    width: thumb_width,
                    height: bounds.height - scrollbar_constants::TRACK_THUMB_GAP * 2.0,
                };

                (track_width, thumb_width, thumb_bounds)
            }
            ScrollbarOrientation::Vertical => {
                let track_height = bounds.height - scrollbar_constants::TRACK_THUMB_GAP * 2.0;
                let scrollable_height = (self.max_scroll - track_height).max(0.0);

                let thumb_height = if scrollable_height <= 0.0 {
                    track_height
                } else {
                    (track_height * track_height / self.max_scroll)
                        .max(scrollbar_constants::THUMB_MIN_SIZE_PX)
                        .min(track_height)
                };

                let clamped_scroll = self.scroll.clamp(0.0, scrollable_height);
                let thumb_y = bounds.y
                    + scrollbar_constants::THUMB_TRACK_EDGE_GAP
                    + (clamped_scroll / scrollable_height.max(1.0)) * (track_height - thumb_height);

                let thumb_bounds = Rectangle {
                    x: bounds.x + scrollbar_constants::THUMB_TRACK_EDGE_GAP,
                    y: thumb_y,
                    width: bounds.width - scrollbar_constants::TRACK_THUMB_GAP * 2.0,
                    height: thumb_height,
                };

                (track_height, thumb_height, thumb_bounds)
            }
        }
    }

    pub(crate) fn get_edge(
        &self,
        position: iced_core::Point,
        thumb_bounds: Rectangle,
    ) -> Option<Edge> {
        let edge_width = scrollbar_constants::EDGE_WIDTH_PX;
        match self.orientation {
            ScrollbarOrientation::Horizontal => {
                if position.x >= thumb_bounds.x && position.x <= thumb_bounds.x + edge_width {
                    Some(Edge::Start)
                } else if position.x >= thumb_bounds.x + thumb_bounds.width - edge_width
                    && position.x <= thumb_bounds.x + thumb_bounds.width
                {
                    Some(Edge::End)
                } else {
                    None
                }
            }
            ScrollbarOrientation::Vertical => {
                if position.y >= thumb_bounds.y && position.y <= thumb_bounds.y + edge_width {
                    Some(Edge::Start)
                } else if position.y >= thumb_bounds.y + thumb_bounds.height - edge_width
                    && position.y <= thumb_bounds.y + thumb_bounds.height
                {
                    Some(Edge::End)
                } else {
                    None
                }
            }
        }
    }

    pub(crate) fn determine_state_at_position(
        &self,
        position: iced_core::Point,
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
}
