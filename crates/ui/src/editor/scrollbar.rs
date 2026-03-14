//! 滚动条模块 - 保留用于未来扩展
//!
//! 此模块包含滚动条相关的数据结构，目前尚未在主程序中使用，
//! 但保留用于未来的滚动条功能实现。

/// 滚动条状态
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ScrollbarState {
    #[default]
    Idle,
    HoverThumb,
    DraggingThumb {
        start_x: f32,
        start_thumb_x: f32,
        bounds_width: f32,
    },
}

/// 滚动条
pub struct Scrollbar {
    pub thumb_width: f32,
    pub edge_width: f32,
    pub state: ScrollbarState,
    pub new_scroll_x: Option<f32>,
    pub thumb_ratio: f32,
}

impl Scrollbar {
    pub fn new(thumb_width: f32) -> Self {
        Self {
            thumb_width,
            edge_width: 5.0,
            state: ScrollbarState::Idle,
            new_scroll_x: None,
            thumb_ratio: 0.0,
        }
    }

    pub fn update_thumb_from_scroll(&mut self, scroll_x: f32, max_scroll: f32) {
        if max_scroll <= 0.0 {
            self.thumb_ratio = 0.0;
            return;
        }
        self.thumb_ratio = (scroll_x / max_scroll).clamp(0.0, 1.0);
    }

    pub fn calculate_scroll_from_ratio(&self, max_scroll: f32) -> f32 {
        self.thumb_ratio * max_scroll
    }

    pub fn thumb_x(&self, bounds_width: f32) -> f32 {
        let available_width = bounds_width - self.thumb_width;
        if available_width <= 0.0 {
            return 0.0;
        }
        self.thumb_ratio * available_width
    }

    pub fn is_mouse_on_thumb(&self, mouse_x: f32, bounds_width: f32) -> bool {
        let thumb_x = self.thumb_x(bounds_width);
        mouse_x >= thumb_x && mouse_x <= thumb_x + self.thumb_width
    }
}

/// 滚动条视图状态
#[derive(Debug, Default)]
pub struct ScrollbarViewState {
    pub state: ScrollbarState,
    pub thumb_ratio: f32,
    pub new_scroll_x: Option<f32>,
    pub thumb_width: f32,
}
