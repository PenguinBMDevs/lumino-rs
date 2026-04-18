use crate::error::Result;
use crate::node::{DATALENGTH_SIZE, DmsCompositeNode, DmsNode, TYPEID_SIZE, create_node};
use crate::node_type::DmsNodeType;
use crate::reader::types::{DmsParseContext, HEADER_SIZE};

/// DMS 文件读取器
pub struct DmsReader;

impl DmsReader {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for DmsReader {
    fn default() -> Self {
        Self::new()
    }
}

impl DmsReader {
    pub fn parse_composite_node(
        &self,
        ctx: &DmsParseContext,
        type_id: DmsNodeType,
        layer: i32,
        start_offset: usize,
        length: usize,
        progress_callback: Option<&dyn Fn(f64)>,
        current_offset: &mut usize,
    ) -> Result<DmsCompositeNode> {
        let mut node = DmsCompositeNode::new(type_id, layer);
        if length == 0 {
            return Ok(node);
        }

        let end_offset = start_offset + length;
        let mut child_offset = start_offset;
        let total_length = ctx.as_slice().len();

        while child_offset < end_offset {
            let child_type_id = self.read_type_id_at(ctx, child_offset)?;
            let child_data_length = self.read_data_length_at(ctx, child_offset + TYPEID_SIZE)?;
            let child_data_start = child_offset + HEADER_SIZE;

            let full_type_id = DmsNodeType::from_parts(child_type_id, layer + 1, Some(&type_id));

            let child = if full_type_id.is_composite() {
                let composite = self.parse_composite_node(
                    ctx,
                    full_type_id,
                    layer + 1,
                    child_data_start,
                    child_data_length,
                    progress_callback,
                    current_offset,
                )?;
                Box::new(composite) as Box<dyn DmsNode>
            } else {
                let data = ctx.slice(child_data_start, child_data_start + child_data_length);
                create_node(full_type_id, layer + 1, data)?
            };

            node.children.push(child);
            child_offset += HEADER_SIZE + child_data_length;
            *current_offset = child_offset;

            if let Some(cb) = progress_callback {
                cb(*current_offset as f64 / total_length as f64);
            }
        }

        Ok(node)
    }

    #[inline]
    pub fn read_type_id_at(&self, ctx: &DmsParseContext, offset: usize) -> Result<u16> {
        let data = ctx.as_slice();
        if offset + TYPEID_SIZE > data.len() {
            return Err(crate::error::DmsError::Corrupted(
                "读取类型 ID 超出数据范围".to_string(),
            ));
        }
        Ok(u16::from_le_bytes([data[offset], data[offset + 1]]))
    }

    #[inline]
    pub fn read_data_length_at(&self, ctx: &DmsParseContext, offset: usize) -> Result<usize> {
        let data = ctx.as_slice();
        if offset + DATALENGTH_SIZE > data.len() {
            return Err(crate::error::DmsError::Corrupted(
                "读取数据长度超出数据范围".to_string(),
            ));
        }
        Ok(u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as usize)
    }
}

mod stream {
    use bytes::Bytes;
    use std::io::Read;

    use crate::error::Result;
    use crate::node::{DATALENGTH_SIZE, DmsNode, TYPEID_SIZE, create_node};
    use crate::node_type::DmsNodeType;

    use super::DmsReader;

    impl DmsReader {
        pub fn read_node<R: Read>(
            &self,
            stream: &mut R,
            layer: i32,
            parent_type: Option<&DmsNodeType>,
        ) -> Result<Box<dyn DmsNode>> {
            let raw_type_id = Self::read_type_id_raw(stream)?;
            let type_id = DmsNodeType::from_parts(raw_type_id, layer, parent_type);
            let data_length = Self::read_data_length(stream)?;

            let mut data = vec![0u8; data_length];
            stream.read_exact(&mut data)?;

            create_node(type_id, layer, Bytes::from(data))
        }

        pub fn read_type_id_raw<R: Read>(stream: &mut R) -> Result<u16> {
            let mut buffer = [0u8; TYPEID_SIZE];
            stream.read_exact(&mut buffer)?;
            Ok(u16::from_le_bytes(buffer))
        }

        pub fn read_data_length<R: Read>(stream: &mut R) -> Result<usize> {
            let mut buffer = [0u8; DATALENGTH_SIZE];
            stream.read_exact(&mut buffer)?;
            Ok(u32::from_le_bytes(buffer) as usize)
        }
    }
}
