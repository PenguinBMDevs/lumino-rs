// DMS 节点数据结构

use std::io::Read;

use bytes::Bytes;

use crate::error::{DmsError, Result};
use crate::node_type::DmsNodeType;
use encoding_rs::GB18030;
use num_bigint::BigInt;

/// 类型 ID 字段大小
pub const TYPEID_SIZE: usize = 2;

/// 数据长度字段大小
pub const DATALENGTH_SIZE: usize = 4;

/// 节点头大小
const HEADER_SIZE: usize = TYPEID_SIZE + DATALENGTH_SIZE;

/// DMS 节点统一接口
pub trait DmsNode: Send + Sync {
    /// 获取节点类型 ID
    fn type_id(&self) -> DmsNodeType;

    /// 获取层级深度（根节点为 -1）
    fn layer(&self) -> i32;

    /// 获取父节点引用（未实现）
    #[deprecated(note = "当前未实现父子关系追踪")]
    fn parent(&self) -> Option<&dyn DmsNode>;

    /// 是否包含数据
    fn has_data(&self) -> bool;

    /// 获取原始字节数据
    fn raw_data(&self) -> &[u8];

    /// 获取数据长度
    fn length(&self) -> usize;

    /// 获取内容类型
    fn content_type(&self) -> &'static str;

    /// 获取显示内容
    fn show_content(&self) -> String;

    /// 获取原始内容对象
    fn content_raw(&self) -> Box<dyn std::any::Any>;

    /// 是否为复合节点
    fn is_composite(&self) -> bool;

    /// 获取子节点列表
    fn children(&self) -> &[Box<dyn DmsNode>];

    /// 获取可变子节点列表
    fn children_mut(&mut self) -> &mut Vec<Box<dyn DmsNode>>;

    /// 获取相对索引
    fn relative_index(&self) -> usize;

    /// 设置相对索引
    fn set_relative_index(&mut self, index: usize);

    /// 获取类型擦除引用
    fn as_any(&self) -> &dyn std::any::Any;
}

/// 原始二进制数据节点
pub struct DmsDataNode {
    /// 节点类型
    pub type_id: DmsNodeType,
    /// 层级深度
    pub layer: i32,
    /// 相对索引
    pub relative_index: usize,
    /// 原始数据
    pub raw_data: Bytes,
    /// 空子节点列表
    empty_children: Vec<Box<dyn DmsNode>>,
}

impl std::fmt::Debug for DmsDataNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DmsDataNode")
            .field("type_id", &self.type_id)
            .field("layer", &self.layer)
            .field("relative_index", &self.relative_index)
            .field("raw_data", &self.raw_data)
            .finish()
    }
}

impl DmsDataNode {
    /// 创建数据节点
    #[inline]
    pub fn new(type_id: DmsNodeType, layer: i32, data: Bytes) -> Self {
        Self {
            type_id,
            layer,
            relative_index: 0,
            raw_data: data,
            empty_children: Vec::new(),
        }
    }

    /// 从流创建节点（会拷贝数据）
    #[inline]
    pub fn from_stream<R: Read>(
        type_id: DmsNodeType,
        layer: i32,
        stream: &mut R,
        length: usize,
    ) -> Result<Self> {
        let mut data = vec![0u8; length];
        stream.read_exact(&mut data)?;
        Ok(Self::new(type_id, layer, Bytes::from(data)))
    }
}

impl DmsNode for DmsDataNode {
    #[inline]
    fn type_id(&self) -> DmsNodeType {
        self.type_id
    }

    #[inline]
    fn layer(&self) -> i32 {
        self.layer
    }

    fn parent(&self) -> Option<&dyn DmsNode> {
        None
    }

    #[inline]
    fn has_data(&self) -> bool {
        true
    }

    #[inline]
    fn raw_data(&self) -> &[u8] {
        &self.raw_data
    }

    #[inline]
    fn length(&self) -> usize {
        self.raw_data.len()
    }

    fn content_type(&self) -> &'static str {
        "binary"
    }

    fn show_content(&self) -> String {
        self.raw_data
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn content_raw(&self) -> Box<dyn std::any::Any> {
        Box::new(self.raw_data.to_vec())
    }

    #[inline]
    fn is_composite(&self) -> bool {
        false
    }

    #[inline]
    fn children(&self) -> &[Box<dyn DmsNode>] {
        &[]
    }

    #[inline]
    fn children_mut(&mut self) -> &mut Vec<Box<dyn DmsNode>> {
        &mut self.empty_children
    }

    #[inline]
    fn relative_index(&self) -> usize {
        self.relative_index
    }

    #[inline]
    fn set_relative_index(&mut self, index: usize) {
        self.relative_index = index;
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// 复合节点（包含子节点）
pub struct DmsCompositeNode {
    /// 节点类型
    pub type_id: DmsNodeType,
    /// 层级深度
    pub layer: i32,
    /// 相对索引
    pub relative_index: usize,
    /// 子节点列表
    pub children: Vec<Box<dyn DmsNode>>,
}

impl std::fmt::Debug for DmsCompositeNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DmsCompositeNode")
            .field("type_id", &self.type_id)
            .field("layer", &self.layer)
            .field("relative_index", &self.relative_index)
            .field("children_count", &self.children.len())
            .finish()
    }
}

impl DmsCompositeNode {
    /// 创建空复合节点
    #[inline]
    pub fn new(type_id: DmsNodeType, layer: i32) -> Self {
        Self {
            type_id,
            layer,
            relative_index: 0,
            children: Vec::new(),
        }
    }

    /// 从数据解析创建（带进度回调）
    pub fn from_data_with_progress(
        type_id: DmsNodeType,
        layer: i32,
        data: Bytes,
        progress_callback: Option<&dyn Fn(f64)>,
    ) -> Result<Self> {
        let length = data.len();
        let mut cursor = std::io::Cursor::new(data);
        Self::from_stream_with_progress(type_id, layer, &mut cursor, length, progress_callback)
    }

    /// 从数据解析创建
    #[inline]
    pub fn from_data(type_id: DmsNodeType, layer: i32, data: Bytes) -> Result<Self> {
        Self::from_data_with_progress(type_id, layer, data, None)
    }

    /// 从流读取创建（带进度回调）
    pub fn from_stream_with_progress<R: Read>(
        type_id: DmsNodeType,
        layer: i32,
        stream: &mut R,
        length: usize,
        progress_callback: Option<&dyn Fn(f64)>,
    ) -> Result<Self> {
        let mut node = Self::new(type_id, layer);

        if length == 0 {
            return Ok(node);
        }

        use crate::reader::DmsReader;
        let reader = DmsReader::new();

        let mut bytes_read = 0usize;

        while bytes_read < length {
            let child = reader.read_node(stream, layer + 1, Some(&type_id))?;
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

    /// 从流读取创建
    #[inline]
    pub fn from_stream<R: Read>(
        type_id: DmsNodeType,
        layer: i32,
        stream: &mut R,
        length: usize,
    ) -> Result<Self> {
        Self::from_stream_with_progress(type_id, layer, stream, length, None)
    }

    /// 计算序列化后总长度
    #[inline]
    pub fn calculate_length(&self) -> usize {
        self.children
            .iter()
            .map(|child| HEADER_SIZE + child.length())
            .sum()
    }
}

impl DmsNode for DmsCompositeNode {
    #[inline]
    fn type_id(&self) -> DmsNodeType {
        self.type_id
    }

    #[inline]
    fn layer(&self) -> i32 {
        self.layer
    }

    fn parent(&self) -> Option<&dyn DmsNode> {
        None
    }

    #[inline]
    fn has_data(&self) -> bool {
        false
    }

    #[inline]
    fn raw_data(&self) -> &[u8] {
        &[]
    }

    #[inline]
    fn length(&self) -> usize {
        self.calculate_length()
    }

    fn content_type(&self) -> &'static str {
        ""
    }

    fn show_content(&self) -> String {
        format!("{} members", self.children.len())
    }

    fn content_raw(&self) -> Box<dyn std::any::Any> {
        let content: Vec<(u16, Box<dyn std::any::Any>)> = self
            .children
            .iter()
            .map(|child| (child.type_id().base_type(), child.content_raw()))
            .collect();
        Box::new(content)
    }

    #[inline]
    fn is_composite(&self) -> bool {
        true
    }

    #[inline]
    fn children(&self) -> &[Box<dyn DmsNode>] {
        &self.children
    }

    #[inline]
    fn children_mut(&mut self) -> &mut Vec<Box<dyn DmsNode>> {
        &mut self.children
    }

    #[inline]
    fn relative_index(&self) -> usize {
        self.relative_index
    }

    #[inline]
    fn set_relative_index(&mut self, index: usize) {
        self.relative_index = index;
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// GB18030 编码字符串节点
pub struct DmsAnsiStringNode {
    /// 基础数据节点
    pub base: DmsDataNode,
}

impl DmsAnsiStringNode {
    /// 创建字符串节点
    #[inline]
    pub fn new(type_id: DmsNodeType, layer: i32, data: Bytes) -> Self {
        Self {
            base: DmsDataNode::new(type_id, layer, data),
        }
    }

    /// 获取解码后的字符串
    pub fn string_data(&self) -> Result<String> {
        let (decoded, _, had_errors) = GB18030.decode(&self.base.raw_data);
        if had_errors {
            return Err(DmsError::Corrupted("无效的 GB18030 编码".to_string()));
        }
        Ok(decoded.to_string())
    }

    /// 设置字符串（自动编码为 GB18030）
    #[inline]
    pub fn set_string_data(&mut self, value: &str) {
        let (encoded, _, _) = GB18030.encode(value);
        self.base.raw_data = Bytes::from(encoded.to_vec());
    }
}

impl DmsNode for DmsAnsiStringNode {
    #[inline]
    fn type_id(&self) -> DmsNodeType {
        self.base.type_id
    }

    #[inline]
    fn layer(&self) -> i32 {
        self.base.layer
    }

    fn parent(&self) -> Option<&dyn DmsNode> {
        None
    }

    #[inline]
    fn has_data(&self) -> bool {
        true
    }

    #[inline]
    fn raw_data(&self) -> &[u8] {
        &self.base.raw_data
    }

    #[inline]
    fn length(&self) -> usize {
        self.base.raw_data.len()
    }

    fn content_type(&self) -> &'static str {
        "string"
    }

    fn show_content(&self) -> String {
        self.string_data().unwrap_or_default()
    }

    fn content_raw(&self) -> Box<dyn std::any::Any> {
        Box::new(self.string_data().unwrap_or_default())
    }

    #[inline]
    fn is_composite(&self) -> bool {
        false
    }

    #[inline]
    fn children(&self) -> &[Box<dyn DmsNode>] {
        &[]
    }

    #[inline]
    fn children_mut(&mut self) -> &mut Vec<Box<dyn DmsNode>> {
        &mut self.base.empty_children
    }

    #[inline]
    fn relative_index(&self) -> usize {
        self.base.relative_index
    }

    #[inline]
    fn set_relative_index(&mut self, index: usize) {
        self.base.relative_index = index;
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// 整数节点（小端字节序，支持任意长度）
pub struct DmsIntegerNode {
    /// 基础数据节点
    pub base: DmsDataNode,
}

impl DmsIntegerNode {
    /// 创建整数节点
    #[inline]
    pub fn new(type_id: DmsNodeType, layer: i32, data: Bytes) -> Self {
        Self {
            base: DmsDataNode::new(type_id, layer, data),
        }
    }

    /// 获取整数值
    #[inline]
    pub fn integer_data(&self) -> BigInt {
        BigInt::from_bytes_le(num_bigint::Sign::Plus, &self.base.raw_data)
    }

    /// 设置整数值
    #[inline]
    pub fn set_integer_data(&mut self, value: &BigInt) {
        let (_, bytes) = value.to_bytes_le();
        self.base.raw_data = Bytes::from(bytes);
    }
}

impl DmsNode for DmsIntegerNode {
    #[inline]
    fn type_id(&self) -> DmsNodeType {
        self.base.type_id
    }

    #[inline]
    fn layer(&self) -> i32 {
        self.base.layer
    }

    fn parent(&self) -> Option<&dyn DmsNode> {
        None
    }

    #[inline]
    fn has_data(&self) -> bool {
        true
    }

    #[inline]
    fn raw_data(&self) -> &[u8] {
        &self.base.raw_data
    }

    #[inline]
    fn length(&self) -> usize {
        self.base.raw_data.len()
    }

    fn content_type(&self) -> &'static str {
        "integer"
    }

    fn show_content(&self) -> String {
        self.integer_data().to_string()
    }

    fn content_raw(&self) -> Box<dyn std::any::Any> {
        Box::new(self.integer_data())
    }

    #[inline]
    fn is_composite(&self) -> bool {
        false
    }

    #[inline]
    fn children(&self) -> &[Box<dyn DmsNode>] {
        &[]
    }

    #[inline]
    fn children_mut(&mut self) -> &mut Vec<Box<dyn DmsNode>> {
        &mut self.base.empty_children
    }

    #[inline]
    fn relative_index(&self) -> usize {
        self.base.relative_index
    }

    #[inline]
    fn set_relative_index(&mut self, index: usize) {
        self.base.relative_index = index;
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// 浮点数节点（支持 f32/f64）
pub struct DmsFloatNode {
    /// 基础数据节点
    pub base: DmsDataNode,
    /// 是否为双精度
    pub is_double: bool,
}

impl DmsFloatNode {
    /// 创建浮点数节点（自动检测精度）
    pub fn new(type_id: DmsNodeType, layer: i32, data: Bytes) -> Result<Self> {
        let mut node = Self {
            base: DmsDataNode::new(type_id, layer, data),
            is_double: true,
        };
        node.parse_format()?;
        Ok(node)
    }

    /// 解析数据格式
    fn parse_format(&mut self) -> Result<()> {
        let data = &self.base.raw_data;

        if data.len() >= HEADER_SIZE {
            let type_field = u16::from_le_bytes([data[0], data[1]]);
            let length_field = u32::from_le_bytes([data[2], data[3], data[4], data[5]]);

            if type_field == 0 {
                // 单精度：总长度 10 字节
                if data.len() == HEADER_SIZE + 4 && length_field == 4 {
                    self.is_double = false;
                    return Ok(());
                }
                // 双精度：总长度 14 字节
                if data.len() == HEADER_SIZE + 8 && length_field == 8 {
                    self.is_double = true;
                    return Ok(());
                }
            }
        }

        Err(DmsError::UnsupportedType("不支持的浮点数格式".to_string()))
    }

    /// 获取浮点数值
    pub fn number_data(&self) -> f64 {
        let data = &self.base.raw_data;

        if self.is_double {
            data.get(HEADER_SIZE..HEADER_SIZE + 8)
                .and_then(|b| b.try_into().ok())
                .map_or(0.0, f64::from_le_bytes)
        } else {
            data.get(HEADER_SIZE..HEADER_SIZE + 4)
                .and_then(|b| b.try_into().ok())
                .map_or(0.0, |b| f32::from_le_bytes(b) as f64)
        }
    }

    /// 设置浮点数值
    pub fn set_number_data(&mut self, value: f64) {
        let mut buffer = vec![0u8; HEADER_SIZE];

        if self.is_double {
            buffer.extend_from_slice(&value.to_le_bytes());
            buffer[2..6].copy_from_slice(&8u32.to_le_bytes());
        } else {
            buffer.extend_from_slice(&(value as f32).to_le_bytes());
            buffer[2..6].copy_from_slice(&4u32.to_le_bytes());
        }

        self.base.raw_data = Bytes::from(buffer);
    }
}

impl DmsNode for DmsFloatNode {
    #[inline]
    fn type_id(&self) -> DmsNodeType {
        self.base.type_id
    }

    #[inline]
    fn layer(&self) -> i32 {
        self.base.layer
    }

    fn parent(&self) -> Option<&dyn DmsNode> {
        None
    }

    #[inline]
    fn has_data(&self) -> bool {
        true
    }

    #[inline]
    fn raw_data(&self) -> &[u8] {
        &self.base.raw_data
    }

    #[inline]
    fn length(&self) -> usize {
        self.base.raw_data.len()
    }

    fn content_type(&self) -> &'static str {
        if self.is_double { "double" } else { "float" }
    }

    fn show_content(&self) -> String {
        self.number_data().to_string()
    }

    fn content_raw(&self) -> Box<dyn std::any::Any> {
        Box::new(self.number_data())
    }

    #[inline]
    fn is_composite(&self) -> bool {
        false
    }

    #[inline]
    fn children(&self) -> &[Box<dyn DmsNode>] {
        &[]
    }

    #[inline]
    fn children_mut(&mut self) -> &mut Vec<Box<dyn DmsNode>> {
        &mut self.base.empty_children
    }

    #[inline]
    fn relative_index(&self) -> usize {
        self.base.relative_index
    }

    #[inline]
    fn set_relative_index(&mut self, index: usize) {
        self.base.relative_index = index;
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// 根据类型创建节点
pub fn create_node(node_type: DmsNodeType, layer: i32, data: Bytes) -> Result<Box<dyn DmsNode>> {
    if node_type.is_composite() {
        Ok(Box::new(DmsCompositeNode::from_data(
            node_type, layer, data,
        )?))
    } else if node_type.is_string() {
        Ok(Box::new(DmsAnsiStringNode::new(node_type, layer, data)))
    } else if node_type.is_integer() {
        Ok(Box::new(DmsIntegerNode::new(node_type, layer, data)))
    } else if node_type.is_float() {
        Ok(Box::new(DmsFloatNode::new(node_type, layer, data)?))
    } else {
        Ok(Box::new(DmsDataNode::new(node_type, layer, data)))
    }
}
