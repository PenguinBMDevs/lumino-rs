//! DMS 文件写入器

use crate::error::Result;
use crate::node::{DmsCompositeNode, DmsNode};
use crate::reader::DMS_MAGIC;
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
            if let Some(composite) = node.as_any().downcast_ref::<DmsCompositeNode>() {
                let length = u32::try_from(composite.calculate_length()).map_err(|_| {
                    crate::error::DmsError::UnsupportedType(
                        "Composite node size exceeds u32 max".into(),
                    )
                })?;
                stream.write_all(&length.to_le_bytes())?;
                self.write_tree(stream, composite)?;
            }
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
