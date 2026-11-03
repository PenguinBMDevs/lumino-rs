//! 工具栏溢出菜单子模块
//!
//! 已按职责拆分为以下子模块：
//! - state:       类型定义（ToolbarGroup, OverflowMenuItem）
//! - interaction: 交互逻辑（计算可见/隐藏分组、折叠优先级）
//! - items:       交互逻辑续（隐藏分组 → 菜单项展开；REF-001 行数纪律拆出）
//! - view:        视图渲染（面板网格、按钮样式、tooltip、定位函数）

pub mod interaction;
mod items;
pub mod state;
pub mod view;

pub use state::*;
pub use view::*;
