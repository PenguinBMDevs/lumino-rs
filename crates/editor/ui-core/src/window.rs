//! 窗口状态管理
//!
//! Window 结构体负责管理窗口主题、最大化状态等。
//! 窗口事件（Event）定义已迁至 lumino-ui-core。

use crate::Theme;
use crate::theme::HIGH_CONTRAST_DISPLAY;

pub use crate::window_event::{Event, TrafficAction};

/// 窗口状态
#[derive(Debug, Clone)]
pub struct Window {
    /// 当前窗口主题
    pub theme: Theme,
    /// 窗口是否最大化
    pub is_maximized: bool,
    /// 窗口是否有焦点
    pub is_focused: bool,
    /// 最近一次帧率（fps）
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
    /// 根据主题名称创建一个窗口状态（未最大化、聚焦、无帧率数据）
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
    /// 根据窗口事件更新窗口状态（主题/最大化/焦点/帧率）
    pub fn update(&mut self, event: Event) {
        match event {
            Event::Theme(theme) => self.theme = get_theme(&theme),
            Event::Maximized(maximized) => self.is_maximized = maximized,
            Event::Focused(focused) => self.is_focused = focused,
            Event::FpsUpdate(fps) => {
                self.fps = Some(fps);
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
