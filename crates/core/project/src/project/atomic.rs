//! 工程保存的原子替换入口。
//!
//! 实现已下沉到 [`lumino_core::io::atomic`]，供工程与形状库共用；
//! 本模块只保留工程内部的短路径引用，避免 save.rs 大改。

pub(crate) use lumino_core::io::atomic::{save_dir_atomic, write_file_atomic};
