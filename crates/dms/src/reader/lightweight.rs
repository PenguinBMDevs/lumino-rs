use bytes::Bytes;
use std::io::Cursor;

use super::data::DmsReader;
use crate::error::Result;
use crate::reader::types::DmsLightweightData;

/// 解析已解压的 DMS 数据（带进度回调）
pub fn parse_dms_data_with_progress(
    data: Bytes,
    progress_callback: Option<&dyn Fn(f64)>,
) -> Result<crate::node::DmsCompositeNode> {
    let reader = DmsReader::new();
    reader.parse_data_with_progress(data, progress_callback)
}

/// 解析已解压的 DMS 数据
#[inline]
pub fn parse_dms_data(data: Bytes) -> Result<crate::node::DmsCompositeNode> {
    parse_dms_data_with_progress(data, None)
}

/// `parse_dms_data` 的别名
pub use parse_dms_data as read_dms_data;

/// 轻量级读取 DMS 文件（只解压，不解析节点树）
pub fn read_dms_lightweight(bytes: &[u8]) -> Result<DmsLightweightData> {
    let reader = DmsReader::new();
    let mut cursor = Cursor::new(bytes);
    let data = reader.read_data(&mut cursor)?;
    Ok(DmsLightweightData::new(data))
}
