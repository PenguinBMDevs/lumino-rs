//! Host 窗口事件处理和 UI 状态管理子模块
//!
//! 拆分说明：
//! - `self::events` — 窗口事件转换、队列处理与 UI 状态门控
//! - `self::keyboard` — 键盘/修饰键/触摸与捏合缩放手势
//! - `self::mouse` — 鼠标输入与光标图标更新

mod events;
mod keyboard;
mod mouse;
