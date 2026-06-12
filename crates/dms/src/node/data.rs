//! 原始二进制数据节点

use std::io::Read;

use bytes::Bytes;

use crate::error::Result;
use crate::node::DmsNode;
use crate::node_type::DmsNodeType;

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
    pub empty_children: Vec<Box<dyn DmsNode>>,
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
