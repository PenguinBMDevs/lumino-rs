//! 协作模块类型定义
//!
//! 所有类型定义在子模块中，此文件仅作为统一入口

// 声明子模块
/// 别名类型模块
pub mod alias;
/// 客户端类型模块
pub mod client;
/// 音符类型模块
pub mod note;
/// 项目类型模块
pub mod project;
/// 用户类型模块
pub mod user;
/// 视图类型模块
pub mod view;

// 从子模块重新导出所有类型
pub use alias::*;
pub use client::*;
pub use note::*;
pub use project::*;
pub use user::*;
pub use view::*;

pub use lumino_midi_loader::MidiEvent;
