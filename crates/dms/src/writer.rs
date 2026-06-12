//! DMS 文件写入器

use crate::constants::DMS_MAGIC;
use crate::error::Result;
use crate::node::{DmsCompositeNode, DmsNode};
use flate2::Compression;
use flate2::write::ZlibEncoder;
use std::io::Write;

/// DMS 文件写入器
pub struct DmsWriter;

impl DmsWriter {
    /// 创建写入器
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// # Errors
    ///
    /// Returns an error if the operation fails.
    /// 将节点树写入流（不含文件头）
    pub fn write_tree<W: Write>(&self, stream: &mut W, root: &DmsCompositeNode) -> Result<()> {
        for child in root.children() {
            self.write_node(stream, child.as_ref())?;
        }
        Ok(())
    }

    /// 写入单个节点
    fn write_node<W: Write>(&self, stream: &mut W, node: &dyn DmsNode) -> Result<()> {
        let type_id = node.type_id().base_type();
        stream.write_all(&type_id.to_le_bytes())?;

        if node.is_composite() {
            let composite = node
                .as_any()
                .downcast_ref::<DmsCompositeNode>()
                .ok_or_else(|| {
                    crate::error::DmsError::UnsupportedType(
                        "is_composite() 返回 true 但 downcast 失败，类型系统不一致".into(),
                    )
                })?;
            let length = u32::try_from(composite.calculate_length()).map_err(|_| {
                crate::error::DmsError::UnsupportedType(
                    "Composite node size exceeds u32 max".into(),
                )
            })?;
            stream.write_all(&length.to_le_bytes())?;
            self.write_tree(stream, composite)?;
        } else {
            // 对于所有数据节点（包括浮点数），写入完整的 raw_data
            // 浮点数节点的 raw_data 包含内部头（6字节）+ 浮点值
            let data = node.raw_data();
            let length = u32::try_from(data.len()).map_err(|_| {
                crate::error::DmsError::UnsupportedType("Node data size exceeds u32 max".into())
            })?;
            stream.write_all(&length.to_le_bytes())?;
            stream.write_all(data)?;
        }

        Ok(())
    }

    /// # Errors
    ///
    /// Returns an error if the operation fails.
    /// 将 DMS 数据写入流（含文件头和压缩）
    pub fn write_to_stream<W: Write>(&self, stream: &mut W, root: &DmsCompositeNode) -> Result<()> {
        let mut buffer = Vec::new();
        self.write_tree(&mut buffer, root)?;

        stream.write_all(DMS_MAGIC)?;
        let buffer_len = u32::try_from(buffer.len()).map_err(|_| {
            crate::error::DmsError::UnsupportedType("DMS file size exceeds u32 max".into())
        })?;
        stream.write_all(&buffer_len.to_le_bytes())?;

        // 使用最高压缩级别，对应 C# 的 CompressionLevel.SmallestSize
        let mut encoder = ZlibEncoder::new(stream, Compression::best());
        encoder.write_all(&buffer)?;
        encoder.finish()?;

        Ok(())
    }

    /// # Errors
    ///
    /// Returns an error if the operation fails.
    /// 序列化节点树为字节（不含文件头）
    pub fn to_bytes(&self, root: &DmsCompositeNode) -> Result<Vec<u8>> {
        let mut buffer = Vec::new();
        self.write_tree(&mut buffer, root)?;
        Ok(buffer)
    }

    /// # Errors
    ///
    /// Returns an error if the operation fails.
    /// 序列化完整 DMS 文件为字节
    pub fn to_file_bytes(&self, root: &DmsCompositeNode) -> Result<Vec<u8>> {
        let mut buffer = Vec::new();
        self.write_to_stream(&mut buffer, root)?;
        Ok(buffer)
    }
}

impl Default for DmsWriter {
    fn default() -> Self {
        Self::new()
    }
}

/// # Errors
///
/// Returns an error if the operation fails.
/// 序列化节点树为字节（不含文件头）
pub fn write_dms_tree(root: &DmsCompositeNode) -> Result<Vec<u8>> {
    let writer = DmsWriter::new();
    writer.to_bytes(root)
}

/// # Errors
///
/// Returns an error if the operation fails.
/// 序列化完整 DMS 文件为字节
pub fn write_dms_file(root: &DmsCompositeNode) -> Result<Vec<u8>> {
    let writer = DmsWriter::new();
    writer.to_file_bytes(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{DmsAnsiStringNode, DmsCompositeNode, DmsIntegerNode, DmsNode};
    use crate::node_type::DmsNodeType;
    use crate::reader::read_dms_file;
    use bytes::Bytes;

    /// 辅助：构建一个简单但完整的 DMS 节点树
    fn build_test_tree() -> DmsCompositeNode {
        // 使用 DmsCompositeNode::new() 创建空的复合节点（不从字节流解析）
        let mut root = DmsCompositeNode::new(DmsNodeType::ROOT, -1);

        // SONG_NAME（DMS 使用 GB18030 编码，ASCII 字符兼容）
        let name = DmsAnsiStringNode::new(DmsNodeType::SONG_NAME, 0, Bytes::from("Test Song"));
        root.children.push(Box::new(name));

        // SONG_PPQN
        let ppqn_data = Bytes::from(vec![0xE0u8, 0x01, 0x00, 0x00]);
        let ppqn = DmsIntegerNode::new(DmsNodeType::SONG_PPQN, 0, ppqn_data);
        root.children.push(Box::new(ppqn));

        // TRACK with NOTE（NOTE_EVENT 也是复合节点）
        let mut track = DmsCompositeNode::new(DmsNodeType::TRACK, 0);
        let note = DmsCompositeNode::new(DmsNodeType::NOTE_EVENT, 1);
        track.children.push(Box::new(note));
        root.children.push(Box::new(track));

        root
    }

    #[test]
    fn test_write_to_bytes() {
        let root = build_test_tree();
        let writer = DmsWriter::new();
        let bytes = writer.to_bytes(&root).expect("写入测试树到字节失败");
        assert!(!bytes.is_empty(), "writer output should not be empty");
        assert!(bytes.len() > 10, "should produce reasonable output size");
    }

    #[test]
    fn test_write_file_roundtrip() {
        let root = build_test_tree();
        let writer = DmsWriter::new();
        let file_bytes = writer.to_file_bytes(&root).expect("生成DMS文件字节失败");

        // 验证文件头包含 magic
        assert_eq!(&file_bytes[..DMS_MAGIC.len()], &DMS_MAGIC[..]);

        // 读回并验证结构
        let read_root = read_dms_file(&file_bytes).expect("读取回写后的DMS文件失败");
        assert_eq!(read_root.type_id(), DmsNodeType::ROOT);
        assert!(
            read_root.children().len() >= 2,
            "should have at least 2 children"
        );
    }

    #[test]
    fn test_roundtrip_preserves_song_name() {
        let root = build_test_tree();
        let file_bytes = DmsWriter::new().to_file_bytes(&root).expect("生成DMS文件字节失败");
        let read_root = read_dms_file(&file_bytes).expect("读取回写后的DMS文件失败");

        // 找到 SONG_NAME 节点
        fn find_song_name(node: &dyn DmsNode) -> Option<String> {
            if node.type_id() == DmsNodeType::SONG_NAME {
                if let Some(s) = node.as_any().downcast_ref::<DmsAnsiStringNode>() {
                    return s.string_data().ok();
                }
            }
            for child in node.children() {
                if let found @ Some(_) = find_song_name(child.as_ref()) {
                    return found;
                }
            }
            None
        }

        let name = find_song_name(&read_root);
        assert_eq!(name, Some("Test Song".to_string()));
    }

    #[test]
    fn test_empty_tree() {
        let root = DmsCompositeNode::new(DmsNodeType::ROOT, -1);
        let result = DmsWriter::new().to_bytes(&root);
        assert!(result.is_ok(), "empty tree should write successfully");
        let bytes = result.expect("空树写入应成功");
        assert!(bytes.is_empty(), "empty tree should produce empty output");
    }
}
