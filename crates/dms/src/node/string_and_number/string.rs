//! GB18030 编码字符串节点

use bytes::Bytes;
use encoding_rs::GB18030;

use crate::error::{DmsError, Result};
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
