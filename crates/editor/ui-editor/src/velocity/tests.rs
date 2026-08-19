//! 力度/Tempo/CC 编辑面板单元测试
//!
//! 从 velocity.rs 主文件拆出，因为主文件超过 400 行。
//!
//! 拆分说明（避免单文件超 400 行）：
//! - `velocity.rs`: 力度点构建测试
//! - `cc.rs`: CC 数据测试
//! - `edit_mode.rs`: EditMode 测试
//! - `tempo.rs`: Tempo 数据与延伸测试
//! - `wheel.rs`: 双向滚轮测试

mod cc;
mod edit_mode;
mod tempo;
mod velocity;
mod wheel;
