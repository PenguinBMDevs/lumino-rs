//! 工具栏按钮尺寸契约（供 `overflow` 的宽度预算使用）
//!
//! 工具栏折叠是按**预估宽度**决策的（渲染前拿不到真实布局），所以按钮几何必须有
//! 唯一来源：`overflow` 只从 [`TOOL_SELECTOR_WIDTH`] / [`TOOL_BUTTON_WIDTH`] 取宽度。
//! **改按钮内边距或图标尺寸 = 同时改预算**，否则会出现「要么白折叠、要么溢出被裁」。
//!
//! 视图已全部迁到 egui（`lumino-ui-egui::chrome::toolbar`），`lumino-ui-egui` 的
//! `layout::TOOL_SELECTOR_W` / `TOOL_BUTTON_W` 必须与本文件保持同值；两处都有单测
//! 锁定（egui 侧 `toolbar_controls_bands`）。

/// 工具选择器图标边长。
const TOOL_SELECTOR_ICON: u32 = 17;
/// 普通工具按钮图标边长。
const TOOL_BUTTON_ICON: u32 = 20;
/// 以上两类按钮的统一内边距（四边相同）。
const TOOL_BUTTON_PADDING: u16 = 4;

/// 工具选择器按钮的固定宽度（图标 + 左右内边距）。
pub const TOOL_SELECTOR_WIDTH: f32 = TOOL_SELECTOR_ICON as f32 + (TOOL_BUTTON_PADDING as f32) * 2.0;

/// 普通工具按钮的固定宽度（图标 + 左右内边距）；「更多」按钮同宽。
pub const TOOL_BUTTON_WIDTH: f32 = TOOL_BUTTON_ICON as f32 + (TOOL_BUTTON_PADDING as f32) * 2.0;
