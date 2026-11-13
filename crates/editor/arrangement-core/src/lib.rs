//! 工程走带的核心逻辑（**框架无关**）。
//!
//! 这里只放不依赖任何 UI 框架的部分：视口状态与选择/移动/框选/橡皮/曲线交互。
//! 旧版 侧（`lumino-ui-editor`）与 egui 侧各自把原生事件转成
//! `ArrangementEvent` 后调用同一份 [`interaction::handle_event`]。

pub mod interaction;
pub mod viewport;

pub use viewport::ArrangementViewport;
