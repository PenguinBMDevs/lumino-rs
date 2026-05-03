//! DMS 节点类型定义

use crate::error::Result;
use crate::node_type::DmsNodeType;

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

/// 根据类型创建节点
pub fn create_node(
    node_type: DmsNodeType,
    layer: i32,
    data: bytes::Bytes,
) -> Result<Box<dyn DmsNode>> {
    use crate::node::{
        DmsAnsiStringNode, DmsCompositeNode, DmsDataNode, DmsFloatNode, DmsIntegerNode,
    };

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
