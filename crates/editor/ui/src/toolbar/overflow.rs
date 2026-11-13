//! 工具栏溢出菜单子模块
//!
//! 已按职责拆分为以下子模块：
//! - state:       类型定义（`ToolbarGroup` / `ToolItem` / `ToolbarOverflow` / `OverflowMenuItem`）
//! - interaction: **宽度预算与折叠决策**（谁先让位、逐项折叠、更多按钮预留位）
//! - items:       折叠结果 → 「更多」菜单项（REF-001 行数纪律拆出）
//!
//! 视图已迁到 egui（`lumino-ui-egui::chrome::overflow`），旧版 侧不再有渲染代码。

pub mod interaction;
mod items;
pub mod state;

#[cfg(test)]
pub use interaction::{MORE_BUTTON_WIDTH, MORE_GAP};
pub use state::*;
