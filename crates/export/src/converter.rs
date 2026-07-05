//! 格式间同步转换工具函数
//!
//! 提供 MIDI、DMS、LMS 等格式间的转换入口。
//! 各子模块按格式职责划分：
//!
//! | 模块 | 职责 |
//! |------|------|
//! | [`midi`] | MIDI 源文件导出（`copy_file_sync`, `export_midi_from_parsed_midi_sync`） |
//! | [`dms`]  | DMS ↔ MIDI 转换（`export_midi_from_dms_sync`, `export_dms_from_midi_sync`） |
//! | [`lms`]  | LMS 格式转换（预留） |
//!
//! # 使用示例
//!
//! ```rust,ignore
//! use lumino_export::converter::export_midi_from_dms_sync;
//! let midi_bytes = export_midi_from_dms_sync(Path::new("song.dms"))?;
//! ```

// 子模块声明——扁平结构
pub mod dms;
pub mod lms;
pub mod midi;

// 公共函数重导出（保持向后兼容）
pub use dms::{export_dms_from_midi_sync, export_midi_from_dms_sync};
pub use midi::{copy_file_sync, export_midi_from_parsed_midi_sync};
