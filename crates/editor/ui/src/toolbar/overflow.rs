//! 工具栏溢出菜单子模块
//!
//! 已按职责拆分为以下子模块：
//! - state:       类型定义（ToolbarGroup, OverflowMenuItem）
//! - interaction: 交互逻辑（计算可见/隐藏分组、展开菜单项列表）
//! - view:        视图渲染（面板网格、按钮样式、tooltip、定位函数）

pub mod interaction;
pub mod state;
pub mod view;

pub use state::*;
pub use view::*;
