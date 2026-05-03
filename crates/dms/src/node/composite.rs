//! 复合节点（包含子节点）

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
