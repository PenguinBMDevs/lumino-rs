use std::io::Cursor;

use crate::{ExportError, ExportResult};

pub fn encode_lmpj<T: serde::Serialize>(value: &T) -> ExportResult<Vec<u8>> {
    let data =
        bincode::serialize(value).map_err(|e| ExportError::Encoding(format!("序列化失败: {e}")))?;
    zstd::stream::encode_all(Cursor::new(data), 3)
        .map_err(|e| ExportError::Encoding(format!("压缩失败: {e}")))
}

pub fn decode_lmpj<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> ExportResult<T> {
    let decoded = zstd::stream::decode_all(Cursor::new(bytes))
        .map_err(|e| ExportError::Encoding(format!("解压失败: {e}")))?;
    bincode::deserialize(&decoded).map_err(|e| ExportError::Encoding(format!("反序列化失败: {e}")))
}
