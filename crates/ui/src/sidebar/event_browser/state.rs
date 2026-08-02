//! 事件浏览器状态与通用类型。
//!
//! 类型定义已下沉到 `lumino_ui_core::sidebar_event`（避免循环依赖），
//! 本模块统一重新导出，保持 `event_browser::state::*` 路径兼容。

pub use super::tree::TreeItem;
pub use lumino_ui_core::sidebar_event::{
    ArchiveKey, EditRequest, EventBrowserState, EventListAction, EventListMenuItem, JumpRequest,
    NoteRef, SelectedItem, TextEventKind,
};
