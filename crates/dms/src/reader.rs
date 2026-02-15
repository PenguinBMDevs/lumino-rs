//! DMS 文件读取器

use bytes::Bytes;

use crate::error::{DmsError, Result};
use crate::node::{DATALENGTH_SIZE, DmsCompositeNode, DmsNode, TYPEID_SIZE, create_node};
use crate::node_type::DmsNodeType;
use crate::utils;
use flate2::read::ZlibDecoder;
use std::io::{Cursor, Read};

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
const HEADER_SIZE: usize = TYPEID_SIZE + DATALENGTH_SIZE;

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

/// DMS 文件读取器
pub struct DmsReader;

impl DmsReader {
    /// 创建读取器
    pub fn new() -> Self {
        Self
    }

    /// 从流中读取并解压 DMS 数据
    pub fn read_data<R: Read>(&self, stream: &mut R) -> Result<Bytes> {
        let mut header = [0u8; MAGIC_LENGTH + 4];
        stream.read_exact(&mut header)?;

        if &header[0..MAGIC_LENGTH] != DMS_MAGIC {
            return Err(DmsError::InvalidMagic);
        }

        let decompressed_length = u32::from_le_bytes([
            header[MAGIC_LENGTH],
            header[MAGIC_LENGTH + 1],
            header[MAGIC_LENGTH + 2],
            header[MAGIC_LENGTH + 3],
        ]) as usize;

        let mut decoder = ZlibDecoder::new(stream);
        let mut decompressed = Vec::with_capacity(decompressed_length);
        decoder.read_to_end(&mut decompressed)?;

        if decompressed.len() != decompressed_length {
            return Err(DmsError::Corrupted(format!(
                "解压长度不匹配: 期望 {}, 实际 {}",
                decompressed_length,
                decompressed.len()
            )));
        }

        Ok(Bytes::from(decompressed))
    }

    /// 解析 DMS 数据为节点树（带进度回调）
    pub fn parse_data_with_progress(
        &self,
        data: Bytes,
        progress_callback: Option<&dyn Fn(f64)>,
    ) -> Result<DmsCompositeNode> {
        let ctx = DmsParseContext::new(data);
        let length = ctx.as_slice().len();
        let mut offset = 0usize;

        self.parse_composite_node(
            &ctx,
            DmsNodeType::ROOT,
            -1,
            offset,
            length,
            progress_callback,
            &mut offset,
        )
    }

    /// 解析 DMS 数据为节点树
    #[inline]
    pub fn parse_data(&self, data: Bytes) -> Result<DmsCompositeNode> {
        self.parse_data_with_progress(data, None)
    }

    /// 解析复合节点（零拷贝）
    fn parse_composite_node(
        &self,
        ctx: &DmsParseContext,
        type_id: DmsNodeType,
        layer: i32,
        start_offset: usize,
        length: usize,
        progress_callback: Option<&dyn Fn(f64)>,
        current_offset: &mut usize,
    ) -> Result<DmsCompositeNode> {
        let mut node = DmsCompositeNode::new(type_id, layer);

        if length == 0 {
            return Ok(node);
        }

        let end_offset = start_offset + length;
        let mut child_offset = start_offset;
        let total_length = ctx.as_slice().len();

        while child_offset < end_offset {
            let child_type_id = self.read_type_id_at(ctx, child_offset)?;
            let child_data_length = self.read_data_length_at(ctx, child_offset + TYPEID_SIZE)?;
            let child_data_start = child_offset + HEADER_SIZE;

            let full_type_id = DmsNodeType::from_parts(child_type_id, layer + 1, Some(&type_id));

            let child = if full_type_id.is_composite() {
                let composite = self.parse_composite_node(
                    ctx,
                    full_type_id,
                    layer + 1,
                    child_data_start,
                    child_data_length,
                    progress_callback,
                    current_offset,
                )?;
                Box::new(composite) as Box<dyn DmsNode>
            } else {
                let data = ctx.slice(child_data_start, child_data_start + child_data_length);
                create_node(full_type_id, layer + 1, data)?
            };

            node.children.push(child);
            child_offset += HEADER_SIZE + child_data_length;
            *current_offset = child_offset;

            if let Some(cb) = progress_callback {
                cb(*current_offset as f64 / total_length as f64);
            }
        }

        Ok(node)
    }

    /// 在指定偏移量读取类型 ID
    #[inline]
    fn read_type_id_at(&self, ctx: &DmsParseContext, offset: usize) -> Result<u16> {
        let data = ctx.as_slice();
        if offset + TYPEID_SIZE > data.len() {
            return Err(DmsError::Corrupted("读取类型 ID 超出数据范围".to_string()));
        }
        Ok(u16::from_le_bytes([data[offset], data[offset + 1]]))
    }

    /// 在指定偏移量读取数据长度
    #[inline]
    fn read_data_length_at(&self, ctx: &DmsParseContext, offset: usize) -> Result<usize> {
        let data = ctx.as_slice();
        if offset + DATALENGTH_SIZE > data.len() {
            return Err(DmsError::Corrupted("读取数据长度超出数据范围".to_string()));
        }
        Ok(u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as usize)
    }

    /// 从流中读取单个节点（流式读取，会分配新内存）
    pub fn read_node<R: Read>(
        &self,
        stream: &mut R,
        layer: i32,
        parent_type: Option<&DmsNodeType>,
    ) -> Result<Box<dyn DmsNode>> {
        let raw_type_id = Self::read_type_id_raw(stream)?;
        let type_id = DmsNodeType::from_parts(raw_type_id, layer, parent_type);
        let data_length = Self::read_data_length(stream)?;

        let mut data = vec![0u8; data_length];
        stream.read_exact(&mut data)?;

        create_node(type_id, layer, Bytes::from(data))
    }

    /// 从流中读取原始类型 ID
    fn read_type_id_raw<R: Read>(stream: &mut R) -> Result<u16> {
        let mut buffer = [0u8; TYPEID_SIZE];
        stream.read_exact(&mut buffer)?;
        Ok(u16::from_le_bytes(buffer))
    }

    /// 从流中读取数据长度
    pub fn read_data_length<R: Read>(stream: &mut R) -> Result<usize> {
        let mut buffer = [0u8; DATALENGTH_SIZE];
        stream.read_exact(&mut buffer)?;
        Ok(u32::from_le_bytes(buffer) as usize)
    }

    /// 从字节数组读取 DMS 文件（带进度回调）
    pub fn read_from_bytes_with_progress(
        &self,
        bytes: &[u8],
        progress_callback: Option<&dyn Fn(f64)>,
    ) -> Result<DmsCompositeNode> {
        let mut cursor = Cursor::new(bytes);
        let data = self.read_data(&mut cursor)?;
        self.parse_data_with_progress(data, progress_callback)
    }

    /// 从字节数组读取 DMS 文件
    #[inline]
    pub fn read_from_bytes(&self, bytes: &[u8]) -> Result<DmsCompositeNode> {
        self.read_from_bytes_with_progress(bytes, None)
    }
}

impl Default for DmsReader {
    fn default() -> Self {
        Self::new()
    }
}

/// 读取 DMS 文件（带进度回调）
pub fn read_dms_file_with_progress(
    bytes: &[u8],
    progress_callback: Option<&dyn Fn(f64)>,
) -> Result<DmsCompositeNode> {
    let reader = DmsReader::new();
    reader.read_from_bytes_with_progress(bytes, progress_callback)
}

/// 读取 DMS 文件
#[inline]
pub fn read_dms_file(bytes: &[u8]) -> Result<DmsCompositeNode> {
    read_dms_file_with_progress(bytes, None)
}

/// 解析已解压的 DMS 数据（带进度回调）
pub fn parse_dms_data_with_progress(
    data: Bytes,
    progress_callback: Option<&dyn Fn(f64)>,
) -> Result<DmsCompositeNode> {
    let reader = DmsReader::new();
    reader.parse_data_with_progress(data, progress_callback)
}

/// 解析已解压的 DMS 数据
#[inline]
pub fn parse_dms_data(data: Bytes) -> Result<DmsCompositeNode> {
    parse_dms_data_with_progress(data, None)
}

/// parse_dms_data 的别名
pub use parse_dms_data as read_dms_data;

// ============================================================================
// 轻量级解析（低内存占用）
// ============================================================================

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

    /// 延迟解析为完整节点树
    pub fn parse_full(&self) -> Result<DmsCompositeNode> {
        let reader = DmsReader::new();
        reader.parse_data(self.data.clone())
    }

    /// 延迟解析为完整节点树（带进度）
    pub fn parse_full_with_progress(
        &self,
        progress_callback: Option<&dyn Fn(f64)>,
    ) -> Result<DmsCompositeNode> {
        let reader = DmsReader::new();
        reader.parse_data_with_progress(self.data.clone(), progress_callback)
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
}

/// 轻量级读取 DMS 文件（只解压，不解析节点树）
pub fn read_dms_lightweight(bytes: &[u8]) -> Result<DmsLightweightData> {
    let reader = DmsReader::new();
    let mut cursor = Cursor::new(bytes);
    let data = reader.read_data(&mut cursor)?;
    Ok(DmsLightweightData::new(data))
}

/// NOTE_EVENT 原始 type_id
const NOTE_EVENT_RAW_TYPE_ID: u16 = 2001;

/// TRACK 原始 type_id
const TRACK_RAW_TYPE_ID: u16 = 1003;

/// SONG_NAME 原始 type_id
const SONG_NAME_RAW_TYPE_ID: u16 = 3;

/// SONG_COPYRIGHT 原始 type_id  
const SONG_COPYRIGHT_RAW_TYPE_ID: u16 = 4;

/// SONG_COMMENT 原始 type_id
const SONG_COMMENT_RAW_TYPE_ID: u16 = 5;

/// SONG_PPQN 原始 type_id
const SONG_PPQN_RAW_TYPE_ID: u16 = 8;

/// WORKING_TIME_SEC 原始 type_id
const WORKING_TIME_SEC_RAW_TYPE_ID: u16 = 0x14;

/// 流式扫描 DMS 文件（边解压边提取元数据，不保留完整解压数据）
pub fn scan_dms_streaming<R: Read>(stream: &mut R) -> Result<DmsScanResult> {
    scan_dms_streaming_with_progress(stream, |_| {})
}

/// 流式扫描 DMS 文件（带进度回调）
pub fn scan_dms_streaming_with_progress<R: Read, F: Fn(f64)>(
    stream: &mut R,
    progress_callback: F,
) -> Result<DmsScanResult> {
    let mut header = [0u8; MAGIC_LENGTH + 4];
    stream.read_exact(&mut header)?;

    if &header[0..MAGIC_LENGTH] != DMS_MAGIC {
        return Err(DmsError::InvalidMagic);
    }

    let decompressed_length = u32::from_le_bytes([
        header[MAGIC_LENGTH],
        header[MAGIC_LENGTH + 1],
        header[MAGIC_LENGTH + 2],
        header[MAGIC_LENGTH + 3],
    ]) as usize;

    let mut decoder = ZlibDecoder::new(stream);
    let mut result = DmsScanResult::default();
    let mut buffer = vec![0u8; 8192];
    let mut offset = 0usize;

    while offset < decompressed_length {
        let bytes_read = decoder
            .read(&mut buffer)
            .map_err(|e| DmsError::Corrupted(format!("解压失败: {}", e)))?;

        if bytes_read == 0 {
            break;
        }

        let chunk = &buffer[..bytes_read];
        let mut chunk_offset = 0;

        while chunk_offset + HEADER_SIZE <= chunk.len() {
            let type_id = u16::from_le_bytes([chunk[chunk_offset], chunk[chunk_offset + 1]]);
            let data_length = u32::from_le_bytes([
                chunk[chunk_offset + 2],
                chunk[chunk_offset + 3],
                chunk[chunk_offset + 4],
                chunk[chunk_offset + 5],
            ]) as usize;

            let data_start = chunk_offset + HEADER_SIZE;
            let data_end = data_start + data_length;

            if data_end > chunk.len() {
                break;
            }

            let node_data = &chunk[data_start..data_end];

            match type_id {
                SONG_NAME_RAW_TYPE_ID => {
                    result.song_name = utils::decode_gb18030(node_data);
                }
                SONG_COPYRIGHT_RAW_TYPE_ID => {
                    result.copyright = utils::decode_gb18030(node_data);
                }
                SONG_COMMENT_RAW_TYPE_ID => {
                    result.comment = utils::decode_gb18030(node_data);
                }
                SONG_PPQN_RAW_TYPE_ID => {
                    result.ppqn = utils::decode_u32_le(node_data);
                }
                WORKING_TIME_SEC_RAW_TYPE_ID => {
                    result.working_time_sec = utils::decode_u64_le(node_data);
                }
                t if t == NOTE_EVENT_RAW_TYPE_ID => {
                    result.total_notes += 1;
                }
                t if t == TRACK_RAW_TYPE_ID => {
                    result.track_count += 1;
                }
                _ => {}
            }

            chunk_offset = data_end;
        }

        offset += bytes_read;
        // 调用进度回调
        let progress = offset as f64 / decompressed_length as f64;
        progress_callback(progress.min(1.0));
    }

    Ok(result)
}
