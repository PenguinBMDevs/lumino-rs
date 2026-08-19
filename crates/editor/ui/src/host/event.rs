//! Host 事件处理模块
//!
//! 拆分说明：
//! - `self::keyboard` — 键盘快捷键处理
//! - `self::input` — 鼠标/触摸输入处理
//! - `self::window` — 窗口事件处理、UI 状态管理
//! - `self::message` — 消息分发与处理

mod input;
mod keyboard;
mod message;
mod window;

use iced_winit::winit;

/// 检查是否按下了 Ctrl 或 Command（macOS）
fn is_ctrl_or_cmd_pressed(modifiers: winit::keyboard::ModifiersState) -> bool {
    modifiers.contains(winit::keyboard::ModifiersState::CONTROL)
        || modifiers.contains(winit::keyboard::ModifiersState::SUPER)
}
