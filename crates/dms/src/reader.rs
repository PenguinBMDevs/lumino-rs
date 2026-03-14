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

// 轻量级解析（低内存占用）

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

// 使用 DmsNodeType 中定义的类型ID常量进行扫描
// 注意：这些值必须与 Domino DMS 文件格式完全匹配

/// 流式扫描 DMS 文件（边解压边提取元数据，不保留完整解压数据）
pub fn scan_dms_streaming<R: Read>(stream: &mut R) -> Result<DmsScanResult> {
    scan_dms_streaming_with_progress(stream, |_| {})
}

/// 流式扫描 DMS 文件（带进度回调）
///
/// 使用滑动窗口机制处理大文件，避免频繁内存分配
/// 跟踪父节点上下文以正确识别嵌套节点类型
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

    // 使用 4MB 缓冲区，减少 I/O 次数和内存移动
    const BUF_SIZE: usize = 4 * 1048576;
    // 最大节点数据大小（用于判断是否需要扩容）
    const MAX_NODE_DATA: usize = 65536;

    let mut buffer: Vec<u8> = vec![0; BUF_SIZE + MAX_NODE_DATA];
    let mut valid_len: usize = 0;
    let mut decompressed_offset: usize = 0;
    let mut last_progress_report: f64 = 0.0;

    // 父节点栈：用于跟踪嵌套上下文
    // 每个元素：(基础类型ID, 结束偏移量)
    let mut parent_stack: Vec<(u16, usize)> = Vec::with_capacity(32);
    // 当前累计偏移量（相对于解压数据的起始位置）
    let mut cumulative_offset: usize = 0;

    // 预计算常量
    let track_base = DmsNodeType::TRACK.base_type();
    let note_event_base = DmsNodeType::NOTE_EVENT.base_type();
    let song_name_base = DmsNodeType::SONG_NAME.base_type();
    let song_copyright_base = DmsNodeType::SONG_COPYRIGHT.base_type();
    let song_comment_base = DmsNodeType::SONG_COMMENT.base_type();
    let song_ppqn_base = DmsNodeType::SONG_PPQN.base_type();
    let working_time_base = DmsNodeType::WORKING_TIME_SEC.base_type();
    let current_vars_base = DmsNodeType::CURRENT_VARS.base_type();
    let midi_out_cfg_base = DmsNodeType::MIDI_OUT_CFG.base_type();
    let key_palette_base = DmsNodeType::KEY_PALETTE.base_type();
    let port_cfg_base = DmsNodeType::PORT_CFG.base_type();

    while decompressed_offset < decompressed_length {
        // 如果有效数据较少，尝试读取更多
        if valid_len < MAX_NODE_DATA {
            let read_target = &mut buffer[valid_len..valid_len + BUF_SIZE];
            match decoder.read(read_target) {
                Ok(0) => {
                    if valid_len == 0 {
                        break;
                    }
                }
                Ok(n) => {
                    valid_len += n;
                    decompressed_offset += n;
                }
                Err(e) => {
                    return Err(DmsError::Corrupted(format!("解压失败: {}", e)));
                }
            }
        }

        let mut parse_offset: usize = 0;

        // 解析缓冲区中的节点
        while parse_offset + HEADER_SIZE <= valid_len {
            let type_id = u16::from_le_bytes([buffer[parse_offset], buffer[parse_offset + 1]]);
            let data_length = u32::from_le_bytes([
                buffer[parse_offset + 2],
                buffer[parse_offset + 3],
                buffer[parse_offset + 4],
                buffer[parse_offset + 5],
            ]) as usize;

            let data_start = parse_offset + HEADER_SIZE;
            let data_end = data_start + data_length;

            // 数据跨越缓冲区边界，需要更多数据
            if data_end > valid_len {
                break;
            }

            // 当前节点在解压数据中的绝对结束位置
            let node_end_offset = cumulative_offset + data_end;

            // 弹出已结束的父节点
            while let Some((_, end_offset)) = parent_stack.last() {
                if node_end_offset > *end_offset {
                    parent_stack.pop();
                } else {
                    break;
                }
            }

            // 获取当前父节点的基础类型
            let current_parent_base = parent_stack.last().map(|(base, _)| *base);

            // 处理节点 - 使用预计算的常量进行快速匹配
            if current_parent_base.is_none() {
                match type_id {
                    t if t == song_name_base => {
                        result.song_name = utils::decode_gb18030(&buffer[data_start..data_end]);
                    }
                    t if t == song_copyright_base => {
                        result.copyright = utils::decode_gb18030(&buffer[data_start..data_end]);
                    }
                    t if t == song_comment_base => {
                        result.comment = utils::decode_gb18030(&buffer[data_start..data_end]);
                    }
                    t if t == song_ppqn_base => {
                        result.ppqn = utils::decode_u32_le(&buffer[data_start..data_end]);
                    }
                    t if t == working_time_base => {
                        result.working_time_sec =
                            utils::decode_u64_le(&buffer[data_start..data_end]);
                    }
                    t if t == track_base => {
                        result.track_count += 1;
                    }
                    _ => {}
                }
            }

            // 检查是否为音符事件（在 TRACK 内部，基础类型 2001）
            if current_parent_base == Some(track_base) && type_id == note_event_base {
                result.total_notes += 1;
            }

            // 快速判断是否为复合节点
            let is_composite = type_id == track_base
                || type_id == current_vars_base
                || type_id == midi_out_cfg_base
                || type_id == key_palette_base
                || type_id == port_cfg_base
                || (current_parent_base == Some(track_base) && (2001..=2019).contains(&type_id));

            if is_composite {
                parent_stack.push((type_id, node_end_offset));
            }

            parse_offset = data_end;
        }

        // 更新累计偏移量
        cumulative_offset += parse_offset;

        // 移动剩余数据到缓冲区开头（仅在必要时执行）
        let remaining = valid_len - parse_offset;
        if remaining > 0 && parse_offset > 0 {
            buffer.copy_within(parse_offset..valid_len, 0);
        }
        valid_len = remaining;

        // 更新父节点栈中的结束偏移量
        for (_, end_offset) in parent_stack.iter_mut() {
            *end_offset -= parse_offset;
        }

        // 限制进度回调频率（每 10% 报告一次）
        let progress = decompressed_offset as f64 / decompressed_length as f64;
        if progress - last_progress_report >= 0.1 || decompressed_offset >= decompressed_length {
            progress_callback(progress.min(1.0));
            last_progress_report = progress;
        }
    }

    Ok(result)
}
