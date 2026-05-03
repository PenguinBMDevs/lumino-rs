//! 复合节点（包含子节点）

use std::io::Read;

use bytes::Bytes;

use crate::error::{DmsError, Result};
use crate::constants::HEADER_SIZE;
use crate::node::DmsNode;
use crate::node_type::DmsNodeType;

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
    #[must_use]
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

    /// # Errors
    ///
    /// Returns an error if the operation fails.
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
    #[must_use]
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
