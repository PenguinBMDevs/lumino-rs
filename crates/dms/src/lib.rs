#![allow(
    clippy::cast_precision_loss,
    clippy::missing_errors_doc,
    clippy::unused_self
)]

// DMS 文件格式解析器 (Domino Music Sequencer)

// 导入依赖
pub mod constants;
pub mod error;
pub mod node;
pub mod node_type;
pub mod reader;
pub mod utils;
pub mod writer;

// 重导出常用类型
pub use bytes::Bytes;
pub use constants::{DATALENGTH_SIZE, DMS_MAGIC, HEADER_SIZE, MAGIC_LENGTH, TYPEID_SIZE};
pub use error::{DmsError, Result};
pub use node::{
    DmsAnsiStringNode, DmsCompositeNode, DmsDataNode, DmsFloatNode,
    DmsIntegerNode, DmsNode,
};
pub use node_type::DmsNodeType;
pub use reader::{
    DmsLightweightData, DmsReader, DmsScanResult,
    parse_dms_data_with_progress, read_dms_data, read_dms_file, read_dms_file_with_progress,
    read_dms_lightweight, scan_dms_streaming, scan_dms_streaming_with_progress,
};
pub use writer::{DmsWriter, write_dms_file, write_dms_tree};
