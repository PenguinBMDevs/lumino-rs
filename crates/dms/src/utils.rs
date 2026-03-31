//! 编码解码工具函数

/// GB18030 解码
pub fn decode_gb18030(data: &[u8]) -> Option<String> {
    if data.is_empty() {
        return None;
    }
    let (decoded, _, had_errors) = encoding_rs::GB18030.decode(data);
    if had_errors {
        None
    } else {
        let s = decoded.to_string();
        if s.is_empty() { None } else { Some(s) }
    }
}

/// 小端 u32 解码
pub fn decode_u32_le(data: &[u8]) -> Option<u32> {
    if data.len() < 4 {
        return None;
    }
    Some(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

/// 小端 u64 解码
pub fn decode_u64_le(data: &[u8]) -> Option<u64> {
    if data.len() < 8 {
        return None;
    }
    Some(u64::from_le_bytes([
        data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
    ]))
}
