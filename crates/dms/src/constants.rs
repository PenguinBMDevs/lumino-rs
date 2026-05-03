//! DMS 常量定义

/// DMS 文件魔数
pub const DMS_MAGIC: &[u8] = b"PortalSequenceData";

/// 魔数字节长度
pub const MAGIC_LENGTH: usize = 18;

/// 类型 ID 字段大小（字节）
pub const TYPEID_SIZE: usize = 2;

/// 数据长度字段大小（字节）
pub const DATALENGTH_SIZE: usize = 4;

/// 节点头大小（字节）
pub const HEADER_SIZE: usize = TYPEID_SIZE + DATALENGTH_SIZE;

/// 扫描缓冲区大小（64KB）
pub const SCAN_BUFFER_SIZE: usize = 65536;
