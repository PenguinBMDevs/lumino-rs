//! Runner 生命周期管理模块
//!
//! 此模块已拆分为多个子模块：
//! - dialog: 对话框事件处理
//! - memory: 内存日志功能
//! - midi: MIDI 重初始化
//! - control_flow: 事件循环控制流
//! - test_mode: 测试模式 FPS 监测

mod control_flow;
mod device_gate;
mod dialog;
mod handler;
mod inner;
mod memory;
mod midi;
mod test_mode;
