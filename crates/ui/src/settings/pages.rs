//! 设置页面模块

pub mod about;
pub mod audio;
pub mod general;
pub mod shortcuts;
pub mod ui_settings;

pub use about::view as about_view;
pub use audio::view as audio_view;
pub use general::view as general_view;
pub use shortcuts::view as shortcuts_view;
pub use ui_settings::view as ui_settings_view;
