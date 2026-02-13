// DMS 文件格式解析器 (Domino Music Sequencer)

#![warn(missing_docs)]

// 导入依赖
pub mod error;
pub mod node;
pub mod node_type;
pub mod reader;
pub mod utils;
pub mod writer;

// 重导出常用类型
pub use bytes::Bytes;
pub use error::{DmsError, Result};
pub use node::{
    DATALENGTH_SIZE, DmsAnsiStringNode, DmsCompositeNode, DmsDataNode, DmsFloatNode,
    DmsIntegerNode, DmsNode, TYPEID_SIZE,
};
pub use node_type::DmsNodeType;
pub use reader::{
    DMS_MAGIC, DmsLightweightData, DmsReader, DmsScanResult, MAGIC_LENGTH,
    parse_dms_data_with_progress, read_dms_data, read_dms_file, read_dms_file_with_progress,
    read_dms_lightweight, scan_dms_streaming, scan_dms_streaming_with_progress,
};
pub use writer::{DmsWriter, write_dms_file, write_dms_tree};
