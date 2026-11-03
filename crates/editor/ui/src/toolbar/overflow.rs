//! 工具栏溢出菜单子模块
//!
//! 已按职责拆分为以下子模块：
//! - state:       类型定义（`ToolbarGroup` / `ToolItem` / `ToolbarOverflow` / `OverflowMenuItem`）
//! - interaction: **宽度预算与折叠决策**（谁先让位、逐项折叠、更多按钮预留位）
//! - items:       折叠结果 → 「更多」菜单项（REF-001 行数纪律拆出）
//! - view:        视图渲染（面板网格、按钮样式、tooltip、定位函数）

pub mod interaction;
mod items;
pub mod state;
pub mod view;

pub use interaction::{MORE_BUTTON_WIDTH, MORE_GAP};
pub use state::*;
pub use view::*;
