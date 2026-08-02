//! 事件浏览器模块。
//!
//! 从 egui 实现中解耦，提供事件浏览器的状态、逻辑与 iced Canvas UI。
//! 结构：
//! - `state`：与 UI 框架无关的状态与请求类型（定义于 ui-core，此处重新导出）
//! - `bar_lookup`：tick ↔ 小节位置转换
//! - `table`：分页、行选择纯逻辑
//! - `tree`：左侧树数据查询
//! - `detail`：按 `SelectedItem` 聚合表格行
//! - `canvas`：iced Canvas 渲染与交互
//! - `edit`：popup 编辑状态

pub mod canvas;
pub mod detail;
pub mod edit;
pub mod state;

mod bar_lookup;
mod table;
mod tree;

pub use canvas::view_event_browser;
pub use detail::EventBrowserData;
pub use state::{
    ArchiveKey, EditRequest, EventBrowserState, EventListAction, EventListMenuItem, JumpRequest,
    NoteRef, SelectedItem, TextEventKind,
};
