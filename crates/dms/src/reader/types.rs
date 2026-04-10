//! DMS 读取器类型定义

use bytes::Bytes;

use crate::node::{DATALENGTH_SIZE, DmsCompositeNode, TYPEID_SIZE};

/// DMS 扫描结果（流式，不保留解压数据）
#[derive(Debug, Default)]
pub struct DmsScanResult {
    /// 轨道数量
    pub track_count: usize,
    /// 总音符数
    pub total_notes: u64,
    /// 歌曲名称
    pub song_name: Option<String>,
    /// 版权信息
    pub copyright: Option<String>,
    /// 歌曲备注
    pub comment: Option<String>,
    /// PPQN（每四分音符脉冲数）
    pub ppqn: Option<u32>,
    /// 工作时间（秒）
    pub working_time_sec: Option<u64>,
}

/// DMS 文件魔数
pub const DMS_MAGIC: &[u8] = b"PortalSequenceData";
/// 魔数长度
pub const MAGIC_LENGTH: usize = 18;

/// 节点头大小
pub const HEADER_SIZE: usize = TYPEID_SIZE + DATALENGTH_SIZE;

/// 解析上下文（零拷贝解析）
#[derive(Clone)]
pub struct DmsParseContext {
    /// 原始数据
    data: Bytes,
}

impl DmsParseContext {
    /// 创建解析上下文
    pub fn new(data: Bytes) -> Self {
        Self { data }
    }

    /// 零拷贝切片
    #[inline]
    pub fn slice(&self, start: usize, end: usize) -> Bytes {
        self.data.slice(start..end)
    }

    /// 获取数据引用
    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }
}

/// 轻量级 DMS 数据结构（零拷贝，低内存）
#[derive(Clone, Debug)]
pub struct DmsLightweightData {
    /// 解压后的原始数据
    pub data: Bytes,
}

impl DmsLightweightData {
    /// 创建轻量级数据
    pub fn new(data: Bytes) -> Self {
        Self { data }
    }

    /// 获取数据大小
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// 扫描顶层节点类型（不递归）
    pub fn scan_top_level_types(&self) -> Vec<(u16, usize, usize)> {
        let data = &self.data;
        let mut result = Vec::new();
        let mut offset = 0usize;

        while offset + HEADER_SIZE <= data.len() {
            let type_id = u16::from_le_bytes([data[offset], data[offset + 1]]);
            let data_length = u32::from_le_bytes([
                data[offset + 2],
                data[offset + 3],
                data[offset + 4],
                data[offset + 5],
            ]) as usize;

            result.push((type_id, data_length, offset + HEADER_SIZE));
            offset += HEADER_SIZE + data_length;
        }

        result
    }

    /// # Errors
    ///
    /// Returns an error if the operation fails.
    /// 解析完整 DMS 节点树
    pub fn parse_full(&self) -> crate::error::Result<DmsCompositeNode> {
        use crate::reader::DmsReader;

        let reader = DmsReader::new();
        reader.parse_data(self.data.clone())
    }
}

/// 文件头信息
pub struct FileHeader {
    pub decompressed_length: usize,
}

/// # Errors
///
/// Returns an error if the operation fails.
/// 读取文件头
pub fn read_file_header<R: std::io::Read>(stream: &mut R) -> crate::error::Result<FileHeader> {
    let mut header = [0u8; MAGIC_LENGTH + 4];
    stream.read_exact(&mut header)?;

    if &header[0..MAGIC_LENGTH] != DMS_MAGIC {
        return Err(crate::error::DmsError::InvalidMagic);
    }

    let decompressed_length = u32::from_le_bytes([
        header[MAGIC_LENGTH],
        header[MAGIC_LENGTH + 1],
        header[MAGIC_LENGTH + 2],
        header[MAGIC_LENGTH + 3],
    ]) as usize;

    Ok(FileHeader {
        decompressed_length,
    })
}
