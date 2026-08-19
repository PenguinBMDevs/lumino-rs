//! lumino-dialog — 对话框窗口管理
//!
//! 提供独立的对话框窗口创建、管理和销毁基础设施。
//! 每个对话框是独立的 winit 窗口，拥有自己的渲染上下文和 UI Host。

pub mod manager;
pub mod window;

#[cfg(target_os = "windows")]
pub(crate) mod platform;

#[cfg(target_os = "windows")]
pub use platform::windows::setup_resize_border;

// 重新导出常用类型，方便外部使用
pub use lumino_ui::host::DialogResult;
pub use lumino_ui::state::root_state::DialogType;
pub use manager::{DialogManager, PendingDialog};
pub use window::DialogWindow;
