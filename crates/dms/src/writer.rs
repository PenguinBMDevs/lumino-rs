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
    pub fn new() -> Self {
        Self
    }

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
                let length = composite.calculate_length() as u32;
                stream.write_all(&length.to_le_bytes())?;
                self.write_tree(stream, composite)?;
            }
        } else {
            let data = node.raw_data();
            let length = data.len() as u32;
            stream.write_all(&length.to_le_bytes())?;
            stream.write_all(data)?;
        }

        Ok(())
    }

    /// 将 DMS 数据写入流（含文件头和压缩）
    pub fn write_to_stream<W: Write>(&self, stream: &mut W, root: &DmsCompositeNode) -> Result<()> {
        let mut buffer = Vec::new();
        self.write_tree(&mut buffer, root)?;

        stream.write_all(DMS_MAGIC)?;
        stream.write_all(&(buffer.len() as u32).to_le_bytes())?;

        let mut encoder = ZlibEncoder::new(stream, Compression::best());
        encoder.write_all(&buffer)?;
        encoder.finish()?;

        Ok(())
    }

    /// 序列化节点树为字节（不含文件头）
    pub fn to_bytes(&self, root: &DmsCompositeNode) -> Result<Vec<u8>> {
        let mut buffer = Vec::new();
        self.write_tree(&mut buffer, root)?;
        Ok(buffer)
    }

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

/// 序列化节点树为字节（不含文件头）
pub fn write_dms_tree(root: &DmsCompositeNode) -> Result<Vec<u8>> {
    let writer = DmsWriter::new();
    writer.to_bytes(root)
}

/// 序列化完整 DMS 文件为字节
pub fn write_dms_file(root: &DmsCompositeNode) -> Result<Vec<u8>> {
    let writer = DmsWriter::new();
    writer.to_file_bytes(root)
}
