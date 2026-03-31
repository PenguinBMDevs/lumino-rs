use std::io::Cursor;

pub fn encode_lmpj<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let data = bincode::serialize(value).map_err(|e| format!("序列化失败: {e}"))?;
    zstd::stream::encode_all(Cursor::new(data), 3).map_err(|e| format!("压缩失败: {e}"))
}

pub fn decode_lmpj<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    let decoded =
        zstd::stream::decode_all(Cursor::new(bytes)).map_err(|e| format!("解压失败: {e}"))?;
    bincode::deserialize(&decoded).map_err(|e| format!("反序列化失败: {e}"))
}
