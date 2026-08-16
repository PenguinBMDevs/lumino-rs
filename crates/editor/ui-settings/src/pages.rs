//! 设置页面模块

pub mod about;
pub mod audio;
pub mod cloud;
pub mod editing;
pub mod general;
pub mod onion_skin;
pub mod palette;
pub mod shortcuts;
pub mod ui_settings;

pub use about::view as about_view;
pub use audio::view as audio_view;
pub use cloud::view as cloud_view;
pub use editing::view as editing_view;
pub use general::view as general_view;
pub use onion_skin::view as onion_skin_view;
pub use palette::view as palette_view;
pub use shortcuts::view as shortcuts_view;
pub use ui_settings::view as ui_settings_view;
