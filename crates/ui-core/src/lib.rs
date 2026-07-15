//! Lumino UI 核心类型
//!
//! 包含 UI crate 的共享基本类型：主题、渲染器、消息类型和子系统事件类型。
//! 这些类型可以被 `lumino-ui` 及其子 crate 依赖，而不会产生循环依赖。
//!
//! 模块声明顺序：事件模块在前，message 在后（message 引用事件类型）。

pub mod app_mode;
pub mod theme;
pub mod settings_event;
pub mod window_event;
pub mod toolbar_event;
pub mod sidebar_event;
pub mod message;
pub mod state;

/// Root 持有的子状态类型（视觉/渲染、MIDI 连接、播放）
pub mod visual_state;
pub mod midi_state;
pub mod playback_state;

/// 窗口状态（Window 结构体）
pub mod window;

/// 共享资源（图标等）
pub mod resources;

pub use message::Message;

/// 使用 Iced 默认主题（内置于 iced_core）。
pub type Theme = iced_core::Theme;

/// 使用 WGPU 渲染器。
pub type Renderer = iced_wgpu::Renderer;

/// 类型安全的 UI 元素，绑定到本 crate 的具体 Message/Theme/Renderer。
pub type Element<'a> = iced_core::Element<'a, Message, Theme, Renderer>;
