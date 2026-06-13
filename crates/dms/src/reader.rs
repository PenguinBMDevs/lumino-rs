//! DMS 文件读取器
//!
//! 该模块已拆分为以下子模块：
//! - `types`: 类型定义（DmsScanResult, `DmsParseContext`, `DmsLightweightData` 等）
//! - `scanner`: 流式扫描器（ScanState, `scan_dms_streaming` 等）

use bytes::Bytes;
use flate2::read::ZlibDecoder;
use std::io::{Cursor, Read};

use crate::constants::{DATALENGTH_SIZE, HEADER_SIZE, TYPEID_SIZE};
use crate::error::{DmsError, Result};
use crate::node::{DmsCompositeNode, DmsNode, create_node};
use crate::node_type::DmsNodeType;

pub mod scanner;
pub mod types;

pub use scanner::{ScanState, scan_dms_streaming, scan_dms_streaming_with_progress};
pub use types::{DmsLightweightData, DmsParseContext, DmsScanResult, FileHeader, read_file_header};

/// DMS 文件读取器
pub struct DmsReader;

/// 复合节点解析参数（byte range + metadata）
struct CompositeNodeParseCtx<'a> {
    type_id: DmsNodeType,
    layer: i32,
    start_offset: usize,
    length: usize,
    progress_callback: Option<&'a dyn Fn(f64)>,
    current_offset: &'a mut usize,
}

impl DmsReader {
    /// 创建读取器
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// # Errors
    ///
    /// Returns an error if the operation fails.
    /// 从流中读取并解压 DMS 数据
    pub fn read_data<R: Read>(&self, stream: &mut R) -> Result<Bytes> {
        use crate::reader::types::read_file_header;

        let header = read_file_header(stream)?;
        let mut decoder = ZlibDecoder::new(stream);
        let mut decompressed = Vec::with_capacity(header.decompressed_length);
        decoder.read_to_end(&mut decompressed)?;

        if decompressed.len() != header.decompressed_length {
            return Err(crate::error::DmsError::Corrupted(format!(
                "解压长度不匹配: 期望 {}, 实际 {}",
                header.decompressed_length,
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

        let mut parse_ctx = CompositeNodeParseCtx {
            type_id: DmsNodeType::ROOT,
            layer: -1,
            start_offset: offset,
            length,
            progress_callback,
            current_offset: &mut offset,
        };
        self.parse_composite_node(&ctx, &mut parse_ctx)
    }

    /// # Errors
    ///
    /// Returns an error if the operation fails.
    /// 解析 DMS 数据为节点树
    #[inline]
    pub fn parse_data(&self, data: Bytes) -> Result<DmsCompositeNode> {
        self.parse_data_with_progress(data, None)
    }

    /// 解析复合节点（零拷贝）
    fn parse_composite_node(
        &self,
        ctx: &DmsParseContext,
        p: &mut CompositeNodeParseCtx<'_>,
    ) -> Result<DmsCompositeNode> {
        let mut node = DmsCompositeNode::new(p.type_id, p.layer);

        if p.length == 0 {
            return Ok(node);
        }

        let end_offset = p.start_offset + p.length;
        let mut child_offset = p.start_offset;
        let total_length = ctx.as_slice().len();

        while child_offset < end_offset {
            let child_type_id = self.read_type_id_at(ctx, child_offset)?;
            let child_data_length = self.read_data_length_at(ctx, child_offset + TYPEID_SIZE)?;
            let child_data_start = child_offset + HEADER_SIZE;

            let full_type_id =
                DmsNodeType::from_parts(child_type_id, p.layer + 1, Some(&p.type_id));

            let child = if full_type_id.is_composite() {
                let mut child_ctx = CompositeNodeParseCtx {
                    type_id: full_type_id,
                    layer: p.layer + 1,
                    start_offset: child_data_start,
                    length: child_data_length,
                    progress_callback: p.progress_callback,
                    current_offset: p.current_offset,
                };
                let composite = self.parse_composite_node(ctx, &mut child_ctx)?;
                Box::new(composite) as Box<dyn DmsNode>
            } else {
                let data = ctx.slice(child_data_start, child_data_start + child_data_length);
                create_node(full_type_id, p.layer + 1, data)?
            };

            node.children.push(child);
            child_offset += HEADER_SIZE + child_data_length;
            *p.current_offset = child_offset;

            if let Some(cb) = p.progress_callback {
                cb(*p.current_offset as f64 / total_length as f64);
            }
        }

        Ok(node)
    }

    /// 在指定偏移量读取类型 ID
    #[inline]
    fn read_type_id_at(&self, ctx: &DmsParseContext, offset: usize) -> Result<u16> {
        let data = ctx.as_slice();
        if offset + TYPEID_SIZE > data.len() {
            return Err(crate::error::DmsError::Corrupted(
                "读取类型 ID 超出数据范围".to_string(),
            ));
        }
        Ok(u16::from_le_bytes([data[offset], data[offset + 1]]))
    }

    /// 在指定偏移量读取数据长度
    #[inline]
    fn read_data_length_at(&self, ctx: &DmsParseContext, offset: usize) -> Result<usize> {
        let data = ctx.as_slice();
        if offset + DATALENGTH_SIZE > data.len() {
            return Err(crate::error::DmsError::Corrupted(
                "读取数据长度超出数据范围".to_string(),
            ));
        }
        Ok(u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as usize)
    }

    /// 从流中解析复合节点（带进度回调）
    pub fn parse_composite_from_stream<R: Read>(
        &self,
        type_id: DmsNodeType,
        layer: i32,
        stream: &mut R,
        length: usize,
        progress_callback: Option<&dyn Fn(f64)>,
    ) -> Result<DmsCompositeNode> {
        let mut node = DmsCompositeNode::new(type_id, layer);

        if length == 0 {
            return Ok(node);
        }

        let mut bytes_read = 0usize;

        while bytes_read < length {
            let child = self.read_node(stream, layer + 1, Some(&type_id))?;
            bytes_read += HEADER_SIZE + child.length();

            if bytes_read > length {
                return Err(DmsError::Corrupted(
                    "子节点总长度超过父节点声明长度".to_string(),
                ));
            }

            node.children.push(child);

            if let Some(cb) = progress_callback {
                cb(bytes_read as f64 / length as f64);
            }
        }

        Ok(node)
    }

    /// 从数据解析复合节点（带进度回调）
    pub fn parse_composite_from_data(
        &self,
        type_id: DmsNodeType,
        layer: i32,
        data: Bytes,
        progress_callback: Option<&dyn Fn(f64)>,
    ) -> Result<DmsCompositeNode> {
        let length = data.len();
        let mut cursor = std::io::Cursor::new(data);
        self.parse_composite_from_stream(type_id, layer, &mut cursor, length, progress_callback)
    }

    /// 从数据解析复合节点
    #[inline]
    pub fn parse_composite_from_bytes(
        &self,
        type_id: DmsNodeType,
        layer: i32,
        data: Bytes,
    ) -> Result<DmsCompositeNode> {
        self.parse_composite_from_data(type_id, layer, data, None)
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

        if type_id.is_composite() {
            let composite =
                self.parse_composite_from_stream(type_id, layer, stream, data_length, None)?;
            return Ok(Box::new(composite));
        }

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

    /// # Errors
    ///
    /// Returns an error if the operation fails.
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

    /// # Errors
    ///
    /// Returns an error if the operation fails.
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

/// # Errors
///
/// Returns an error if the operation fails.
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

/// # Errors
///
/// Returns an error if the operation fails.
/// 解析已解压的 DMS 数据
#[inline]
pub fn parse_dms_data(data: Bytes) -> Result<DmsCompositeNode> {
    parse_dms_data_with_progress(data, None)
}

/// `parse_dms_data` 的别名
pub use parse_dms_data as read_dms_data;

/// # Errors
///
/// Returns an error if the operation fails.
/// 轻量级读取 DMS 文件（只解压，不解析节点树）
pub fn read_dms_lightweight(bytes: &[u8]) -> Result<DmsLightweightData> {
    let reader = DmsReader::new();
    let mut cursor = Cursor::new(bytes);
    let data = reader.read_data(&mut cursor)?;
    Ok(DmsLightweightData::new(data))
}
