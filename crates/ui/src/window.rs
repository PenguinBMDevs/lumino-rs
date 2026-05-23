use crate::statusbar::performance::PerfData;
use crate::{Message, Theme};

#[derive(Debug, Clone)]
pub enum Event {
    Theme(String),
    Maximized(bool),
    Focused(bool),
    TrafficAction(TrafficAction),
    Drag,
    ToggleMaximize,
    Close,
    FpsUpdate(f32),
    PerfUpdate(PerfData),
}

#[derive(Debug, Clone)]
pub enum TrafficAction {
    Minimize,
    ToggleMaximize,
    Close,
}

impl Event {
    pub const fn theme(r: String) -> Message {
        Message::Window(Self::Theme(r))
    }
    pub const fn maximized(r: bool) -> Message {
        Message::Window(Self::Maximized(r))
    }
    pub const fn focused(r: bool) -> Message {
        Message::Window(Self::Focused(r))
    }
    pub fn traffic_action(action: &TrafficAction) -> Message {
        Message::Window(Self::TrafficAction(action.clone()))
    }
    pub const fn drag() -> Message {
        Message::Window(Self::Drag)
    }
    pub const fn toggle_maximize() -> Message {
        Message::Window(Self::ToggleMaximize)
    }
    pub const fn close() -> Message {
        Message::Window(Self::Close)
    }
    pub const fn fps_update(fps: f32) -> Message {
        Message::Window(Self::FpsUpdate(fps))
    }
    pub fn perf_update(data: PerfData) -> Message {
        Message::Window(Self::PerfUpdate(data))
    }
}

#[derive(Debug, Clone)]
pub struct Window {
    pub theme: Theme,
    pub is_maximized: bool,
    pub is_focused: bool,
    pub fps: Option<f32>,
}

fn get_theme(theme: &str) -> Theme {
    Theme::ALL
        .iter()
        .find(|t| t.to_string() == theme)
        .cloned()
        .unwrap_or(Window::default_theme())
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
