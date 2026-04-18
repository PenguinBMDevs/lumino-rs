//! 整数节点（小端字节序，支持任意长度）

use bytes::Bytes;
use num_bigint::BigInt;

use crate::node::{DmsDataNode, DmsNode};
use crate::node_type::DmsNodeType;

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
