//! 协作模块类型定义
//!
//! 所有类型定义在子模块中，此文件仅作为统一入口

// 声明子模块
pub mod alias;
pub mod client;
pub mod note;
pub mod project;
pub mod user;
pub mod view;

// 从子模块重新导出所有类型
pub use alias::*;
pub use client::*;
pub use note::*;
pub use project::*;
pub use user::*;
pub use view::*;

pub use lumino_midi_loader::MidiEvent;
