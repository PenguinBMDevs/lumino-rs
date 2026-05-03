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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{DmsAnsiStringNode, DmsCompositeNode};
    use bytes::Bytes;

    #[test]
    fn test_create_composite_node() {
        let node = create_node(DmsNodeType::ROOT, -1, Bytes::new()).unwrap();
        assert!(node.is_composite());
        assert_eq!(node.type_id(), DmsNodeType::ROOT);
        assert_eq!(node.layer(), -1);
        assert!(node.children().is_empty());
    }

    #[test]
    fn test_create_string_node() {
        let data = Bytes::from("Hello DMS");
        let node = create_node(DmsNodeType::SONG_NAME, 0, data).unwrap();
        assert!(!node.is_composite());
        assert!(node.type_id().is_string());
        assert_eq!(node.type_id(), DmsNodeType::SONG_NAME);
        assert_eq!(node.layer(), 0);
        assert!(node.has_data());
    }

    #[test]
    fn test_create_integer_node() {
        let data = Bytes::from(&[42u8, 0, 0, 0][..]);
        let node = create_node(DmsNodeType::SONG_PPQN, 1, data).unwrap();
        assert!(!node.is_composite());
        assert!(node.type_id().is_integer());
        assert_eq!(node.type_id(), DmsNodeType::SONG_PPQN);
    }

    #[test]
    fn test_create_float_node_invalid_data() {
        // 不合法的浮点数格式应返回错误
        let data = Bytes::from(&[0u8, 0, 0, 0, 0, 0, 0, 0][..]);
        let node = create_node(DmsNodeType::TEMPO_VALUE, 0, data);
        assert!(node.is_err(), "invalid float data should return error");
    }

    #[test]
    fn test_node_type_id_consistency() {
        assert_eq!(DmsNodeType::ROOT.0, 0x0000);
        assert_eq!(DmsNodeType::SONG_NAME.0, 1000);
        assert_eq!(DmsNodeType::TRACK.0, 1003);
        assert_eq!(DmsNodeType::NOTE_EVENT.base_type(), 2001);
    }

    #[test]
    fn test_composite_node_children_push() {
        let mut root = DmsCompositeNode::new(DmsNodeType::ROOT, -1);

        let child_data = Bytes::from("Track 1");
        let child = create_node(DmsNodeType::TRACK_NAME, 0, child_data).unwrap();
        root.children.push(child);

        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].type_id(), DmsNodeType::TRACK_NAME);

        // 清空并验证
        root.children.clear();
        assert_eq!(root.children.len(), 0);
    }

    #[test]
    fn test_dms_node_trait_basics() {
        let data = Bytes::from(&[10u8, 0, 0, 0][..]);
        let node = create_node(DmsNodeType::NOTE_VELOCITY, 0, data).unwrap();

        assert!(node.has_data());
        assert_eq!(node.raw_data(), &[10u8, 0, 0, 0]);
        assert_eq!(node.length(), 4);
        assert_eq!(node.relative_index(), 0);
        assert!(node.children().is_empty());
    }

    #[test]
    fn test_roundtrip_simple_tree() {
        use crate::writer::DmsWriter;

        let mut root = DmsCompositeNode::new(DmsNodeType::ROOT, -1);

        let track = DmsCompositeNode::new(DmsNodeType::TRACK, 0);
        root.children.push(Box::new(track));

        let name_data = Bytes::from("test song");
        let name = DmsAnsiStringNode::new(DmsNodeType::SONG_NAME, 0, name_data);
        root.children.push(Box::new(name));

        let writer = DmsWriter::new();
        let mut buf = Vec::new();
        use std::io::Cursor;
        writer.write_tree(&mut Cursor::new(&mut buf), &root).unwrap();
        assert!(!buf.is_empty(), "written data should not be empty");
    }
}
