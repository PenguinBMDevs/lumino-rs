//! 窗口状态管理
//!
//! Window 结构体负责管理窗口主题、最大化状态等。
//! 窗口事件（Event）定义已迁至 lumino-ui-core。

use crate::theme::HIGH_CONTRAST_DISPLAY;
use crate::Theme;

pub use crate::window_event::{Event, TrafficAction};

/// 窗口状态
#[derive(Debug, Clone)]
pub struct Window {
    pub theme: Theme,
    pub is_maximized: bool,
    pub is_focused: bool,
    pub fps: Option<f32>,
}

fn get_theme(theme: &str) -> Theme {
    if theme == HIGH_CONTRAST_DISPLAY {
        crate::theme::set_high_contrast(true);
        crate::theme::hc_theme()
    } else {
        crate::theme::set_high_contrast(false);
        Theme::ALL
            .iter()
            .find(|t| t.to_string() == theme)
            .cloned()
            .unwrap_or(Window::default_theme())
    }
}

impl Window {
    pub fn new(theme: &str) -> Self {
        Self {
            theme: get_theme(theme),
            is_maximized: false,
            is_focused: true,
            fps: None,
        }
    }
    fn default_theme() -> Theme {
        Theme::TokyoNightStorm
    }
    pub fn update(&mut self, event: Event) {
        match event {
            Event::Theme(r) => self.theme = get_theme(&r),
            Event::Maximized(r) => self.is_maximized = r,
            Event::Focused(r) => self.is_focused = r,
            Event::FpsUpdate(v) => {
                self.fps = Some(v);
            }
            Event::PerfUpdate(_) => {
                // PerfUpdate 由 Root 直接处理，不需要更新 Window 状态
            }
            Event::TrafficAction(_) | Event::Drag | Event::ToggleMaximize | Event::Close => {
                // 这些事件由 Host 处理，不需要更新 Window 状态
            }
        }
    }
}
