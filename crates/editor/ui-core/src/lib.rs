//! Lumino UI 核心类型
//!
//! 包含 UI crate 的共享基本类型：主题、渲染器、消息类型和子系统事件类型。
//! 这些类型可以被 `lumino-ui` 及其子 crate 依赖，而不会产生循环依赖。
//!
//! 模块声明顺序：事件模块在前，message 在后（message 引用事件类型）。

// 允许 crate 内部用 `lumino_ui_core::...` 绝对路径引用自身类型
// （几何/主题从 旧版 迁出后，跨 crate 与 crate 内共用同一套路径）。
extern crate self as lumino_ui_core;

/// 应用模式（编辑器/瀑布流）
pub mod app_mode;
/// 工具栏按钮描述配置（按钮名 + i18n 解释说明占位）
pub mod button_descs;
pub mod color;
/// 本地几何与颜色类型（替代 `lumino_ui_core::geom::{Point, Size, Rectangle, Color, mouse}`）
pub mod geom;
/// 全局 UI 字体入口（UI-001：统一字体绑定 + 自定义字体文件加载）
pub mod help;
pub mod message;
/// 洋葱皮显示范围模式（工具栏下拉；工程文件持久化共用同一字符串口径）
pub mod onion_skin_mode;
/// 设置面板事件（定义已迁到 `lumino-message`，此处只保持原路径再导出）
pub use lumino_message::settings_event;
pub mod sidebar_event;
/// 各功能子系统的状态类型模块
pub mod state;
pub mod theme;
pub mod toolbar_event;
pub mod window_event;

/// Root 持有的子状态类型（视觉/渲染状态；播放/MIDI 连接状态已上移至 lumino-ui::state）
pub mod visual_state;
/// 纯函数输入校验（无 旧版 依赖）
pub mod widget;
/// 窗口状态（Window 结构体）
pub mod window;

/// 共享资源（图标等）
pub mod resources;

pub mod constants;

pub use message::Message;

/// 应用主题（本地实现，见 [`theme`]）。
pub use theme::Theme;
