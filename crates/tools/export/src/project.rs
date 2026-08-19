//! 工程文件格式兼容层
//!
//! 核心类型已迁移到 `lumino-core`，本模块保留旧版 LMPJ 兼容加载、
//! 文件夹工程 `.lmpj` 入口读写与 Runner 便捷扩展
//! （`LuminoProject -> ParsedMidi`）。素材（.lmmaterial）见 `material` 模块。
//!
//! 该模块已拆分为以下子模块：
//! - `entry`: 文件夹工程 `.lmpj` 入口读写（save/load image 元数据）
//! - `loader`: 工程加载逻辑（文件夹 / 归档 / 入口 / 旧版 LMPJ 识别）
//! - `midi`: `LuminoProject -> ParsedMidi` 转换

use serde::{Deserialize, Serialize};

mod entry;
mod loader;
mod midi;

pub use lumino_project::project::*;

// 重新导出核心保存函数，保持 `lumino_export::project::save_to_archive` 等路径可用
pub use lumino_project::project::save::{save_to_archive, save_to_folder};

pub use entry::{load_project_image_metadata, save_project_to_folder_with_entry};
pub use loader::load_project;
pub use midi::project_to_parsed_midi;

/// 文件夹工程入口文件内容
///
/// `.lmpj` 文件作为入口，指向同目录下的同名数据文件夹。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LuminoEntryFile {
    /// 文件格式版本号
    pub version: u32,
    /// 工程存储格式标识（如 "folder"）
    pub format: String,
    /// 数据文件夹名称
    pub data_folder: String,
}

impl LuminoEntryFile {
    /// 创建默认文件夹入口
    pub fn folder(data_folder: impl Into<String>) -> Self {
        Self {
            version: 1,
            format: "folder".into(),
            data_folder: data_folder.into(),
        }
    }
}

impl From<lumino_core::CoreError> for crate::ExportError {
    fn from(err: lumino_core::CoreError) -> Self {
        match err {
            lumino_core::CoreError::Io(e) => crate::ExportError::Io(e),
            lumino_core::CoreError::Serialization(s) => crate::ExportError::Encoding(s),
            lumino_core::CoreError::Compression(s) => crate::ExportError::Compression(s),
            lumino_core::CoreError::FileFormat(s) => crate::ExportError::FileFormat(s),
            lumino_core::CoreError::MidiParse(s) => crate::ExportError::MidiParse(s),
            lumino_core::CoreError::InvalidArgument(s) => crate::ExportError::InvalidData(s),
            _ => crate::ExportError::Encoding(err.to_string()),
        }
    }
}

#[cfg(test)]
mod tests;
