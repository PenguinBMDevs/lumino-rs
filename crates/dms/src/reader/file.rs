use bytes::Bytes;
use std::io::{Cursor, Read};

use crate::error::Result;
use crate::reader::types::read_file_header;
use flate2::read::ZlibDecoder;

use crate::reader::data::DmsReader;

impl DmsReader {
    /// 从流中读取并解压 DMS 数据
    pub fn read_data<R: Read>(&self, stream: &mut R) -> Result<Bytes> {
        let header = read_file_header(stream)?;
        let mut decoder = ZlibDecoder::new(stream);
        let mut decompressed = Vec::with_capacity(header.decompressed_length);
        decoder.read_to_end(&mut decompressed)?;

        if decompressed.len() != header.decompressed_length {
            return Err(crate::error::DmsError::Corrupted(format!(
                "解压长度不匹配: 期望 {}, 实际 {}",
                header.decompressed_length,
                decompressed.len()
            )));
        }

        Ok(Bytes::from(decompressed))
    }

    /// 解析 DMS 数据为节点树（带进度回调）
    pub fn parse_data_with_progress(
        &self,
        data: Bytes,
        progress_callback: Option<&dyn Fn(f64)>,
    ) -> Result<crate::node::DmsCompositeNode> {
        use crate::reader::types::DmsParseContext;

        let ctx = DmsParseContext::new(data);
        let length = ctx.as_slice().len();
        let mut offset = 0usize;

        self.parse_composite_node(
            &ctx,
            crate::node_type::DmsNodeType::ROOT,
            -1,
            offset,
            length,
            progress_callback,
            &mut offset,
        )
    }

    /// 解析 DMS 数据为节点树
    #[inline]
    pub fn parse_data(&self, data: Bytes) -> Result<crate::node::DmsCompositeNode> {
        self.parse_data_with_progress(data, None)
    }

    /// 从字节数组读取 DMS 文件（带进度回调）
    pub fn read_from_bytes_with_progress(
        &self,
        bytes: &[u8],
        progress_callback: Option<&dyn Fn(f64)>,
    ) -> Result<crate::node::DmsCompositeNode> {
        let mut cursor = Cursor::new(bytes);
        let data = self.read_data(&mut cursor)?;
        self.parse_data_with_progress(data, progress_callback)
    }

    /// 从字节数组读取 DMS 文件
    #[inline]
    pub fn read_from_bytes(&self, bytes: &[u8]) -> Result<crate::node::DmsCompositeNode> {
        self.read_from_bytes_with_progress(bytes, None)
    }
}

/// 读取 DMS 文件（带进度回调）
pub fn read_dms_file_with_progress(
    bytes: &[u8],
    progress_callback: Option<&dyn Fn(f64)>,
) -> Result<crate::node::DmsCompositeNode> {
    let reader = DmsReader::new();
    reader.read_from_bytes_with_progress(bytes, progress_callback)
}

/// 读取 DMS 文件
#[inline]
pub fn read_dms_file(bytes: &[u8]) -> Result<crate::node::DmsCompositeNode> {
    read_dms_file_with_progress(bytes, None)
}
