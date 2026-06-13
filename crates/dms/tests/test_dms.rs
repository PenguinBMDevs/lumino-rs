//! DMS 解析器测试

use lumino_dms::{Bytes, DmsNode, DmsNodeType, DmsReader, DmsWriter};

#[test]
fn test_magic_constant() {
    assert_eq!(lumino_dms::DMS_MAGIC, b"PortalSequenceData");
    assert_eq!(lumino_dms::MAGIC_LENGTH, 18);
}

#[test]
fn test_node_type_constants() {
    assert_eq!(DmsNodeType::ROOT.0, 0x0000);
    // 注意：以下值必须与 Domino DMS 文件格式完全匹配
    assert_eq!(DmsNodeType::SONG_NAME.0, 1000);
    assert_eq!(DmsNodeType::SONG_COPYRIGHT.0, 1001);
    assert_eq!(DmsNodeType::SONG_COMMENT.0, 1019);
    assert_eq!(DmsNodeType::SONG_PPQN.0, 1002);
    assert_eq!(DmsNodeType::TRACK.0, 1003);
    assert_eq!(DmsNodeType::NOTE_EVENT.base_type(), 2001);
}

#[test]
fn test_node_type_is_composite() {
    assert!(DmsNodeType::ROOT.is_composite());
    assert!(DmsNodeType::TRACK.is_composite());
    assert!(DmsNodeType::NOTE_EVENT.is_composite());
    assert!(!DmsNodeType::SONG_NAME.is_composite());
    assert!(!DmsNodeType::SONG_PPQN.is_composite());
}

#[test]
fn test_node_type_is_string() {
    assert!(DmsNodeType::SONG_NAME.is_string());
    assert!(DmsNodeType::TRACK_NAME.is_string());
    assert!(!DmsNodeType::SONG_PPQN.is_string());
    assert!(!DmsNodeType::TRACK.is_string());
}

#[test]
fn test_node_type_is_integer() {
    assert!(DmsNodeType::SONG_PPQN.is_integer());
    assert!(DmsNodeType::NOTE_VELOCITY.is_integer());
    assert!(!DmsNodeType::SONG_NAME.is_integer());
}

#[test]
fn test_node_type_is_float() {
    assert!(DmsNodeType::TEMPO_VALUE.is_float());
    assert!(DmsNodeType::CONTROL_VALUE.is_float());
    assert!(!DmsNodeType::SONG_PPQN.is_float());
}

#[test]
fn test_node_type_from_parts() {
    let root_type = DmsNodeType::from_parts(0, -1, None);
    assert_eq!(root_type.0, 0);

    let track_type = DmsNodeType::from_parts(1003, 0, Some(&DmsNodeType::ROOT));
    assert_eq!(track_type.0, 1003);
}

#[test]
fn test_create_simple_data_node() {
    use lumino_dms::DmsDataNode;

    let data = Bytes::from(vec![0x01, 0x02, 0x03, 0x04]);
    let node = DmsDataNode::new(DmsNodeType::SONG_PPQN, 1, data.clone());

    assert_eq!(node.type_id().0, DmsNodeType::SONG_PPQN.0);
    assert_eq!(node.layer(), 1);
    assert!(node.has_data());
    assert_eq!(node.raw_data(), &data[..]);
    assert_eq!(node.length(), 4);
    assert_eq!(node.content_type(), "binary");
    assert!(!node.is_composite());
    assert!(node.children().is_empty());
}

#[test]
fn test_create_composite_node() {
    use lumino_dms::DmsCompositeNode;

    let node = DmsCompositeNode::new(DmsNodeType::ROOT, -1);

    assert_eq!(node.type_id().0, DmsNodeType::ROOT.0);
    assert_eq!(node.layer(), -1);
    assert!(!node.has_data());
    assert!(node.is_composite());
    assert!(node.children().is_empty());
}

#[test]
fn test_integer_node() {
    use lumino_dms::DmsIntegerNode;

    // 使用原始数据创建（小端序）
    let data = Bytes::from(vec![0x7B, 0x00, 0x00, 0x00]); // 123 in little-endian
    let node = DmsIntegerNode::new(DmsNodeType::SONG_PPQN, 1, data);

    let value = node.integer_data();
    assert_eq!(value, 123u32.into());
    assert_eq!(node.content_type(), "integer");
}

#[test]
fn test_roundtrip_simple() {
    // 创建带有一个子节点的简单树
    use lumino_dms::{DmsCompositeNode, DmsDataNode};

    let mut root = DmsCompositeNode::new(DmsNodeType::ROOT, -1);
    let data = Bytes::from(vec![0x01, 0x02, 0x03, 0x04]);
    let child = DmsDataNode::new(DmsNodeType::SONG_PPQN, 0, data);

    root.children.push(Box::new(child));

    // 写入字节
    let writer = DmsWriter::new();
    let tree_bytes = writer.to_bytes(&root).expect("写入测试树到字节失败");

    // 读回数据
    let reader = DmsReader::new();
    let parsed = reader
        .parse_data(Bytes::from(tree_bytes))
        .expect("解析回读的DMS数据失败");

    assert_eq!(parsed.children.len(), 1);
    assert_eq!(parsed.children[0].type_id().0, DmsNodeType::SONG_PPQN.0);
    assert_eq!(parsed.children[0].raw_data(), &[0x01, 0x02, 0x03, 0x04]);
}

#[test]
fn test_invalid_magic() {
    use lumino_dms::DmsReader;

    let invalid_data = b"InvalidMagicData1234";
    let reader = DmsReader::new();
    let result = reader.read_from_bytes(invalid_data);

    assert!(result.is_err());
}

#[test]
fn test_scan_dms_streaming() {
    use lumino_dms::scan_dms_streaming;
    use std::io::Cursor;

    // 创建一个简单的 DMS 文件数据（包含魔数和解压长度）
    let mut data = Vec::new();
    data.extend_from_slice(lumino_dms::DMS_MAGIC); // 魔数
    data.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // 解压长度（0）

    let mut cursor = Cursor::new(data);
    let result = scan_dms_streaming(&mut cursor);

    assert!(result.is_ok());
    let scan_result = result.expect("流式扫描DMS文件失败");
    assert_eq!(scan_result.track_count, 0);
    assert_eq!(scan_result.total_notes, 0);
}
