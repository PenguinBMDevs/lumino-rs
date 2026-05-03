//! 字符串节点和数值节点

use bytes::Bytes;
use encoding_rs::GB18030;
use num_bigint::BigInt;

use crate::error::{DmsError, Result};
use crate::constants::HEADER_SIZE;
use crate::node::{DmsDataNode, DmsNode};
use crate::node_type::DmsNodeType;

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

    /// # Errors
    ///
    /// Returns an error if the operation fails.
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
        self.string_data().unwrap_or_else(|e| {
            tracing::warn!("GB18030 解码失败: {}", e);
            String::new()
        })
    }

    fn content_raw(&self) -> Box<dyn std::any::Any> {
        Box::new(self.string_data().unwrap_or_else(|e| {
            tracing::warn!("GB18030 解码失败: {}", e);
            String::new()
        }))
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
    /// # Errors
    ///
    /// Returns an error if the operation fails.
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
                .map_or(0.0, |b| f64::from(f32::from_le_bytes(b)))
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
