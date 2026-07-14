//! 对话框管理器（重导出至 lumino-dialog crate）
//!
//! 对话框管理逻辑已抽取到独立的 `lumino-dialog` crate。
//! 此文件作为向后兼容的薄层，保留所有 pub 类型的导入路径不变。

pub use lumino_dialog::DialogManager;
pub use lumino_dialog::DialogResult;
pub use lumino_dialog::DialogType;
