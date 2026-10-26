//! 形状分享：一行可粘贴文本（DRAW-034）
//!
//! 格式与工程文件、`.lmshapes` 都不同：
//!
//! ```text
//! LUMINO1:<base64(zstd(JSON))>
//! ```
//!
//! JSON 内的坐标单位是 **beats**（不是 tick），因此同一段文本可以在 PPQ 480 与
//! 960 的工程之间往返而不改变音乐含义。编码/解码都必须显式传入 PPQ。
//!
//! 安全边界：前缀/版本先判、忽略空白与换行、输入与解压后长度上限、逐形状校验；
//! 所有失败返回 [`ShareError`]，绝不 panic。

use std::io::{Cursor, Read};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::ShapeModel;

mod convert;
#[cfg(test)]
mod tests;

/// 分享文本前缀（版本号在冒号前）。
pub const PREFIX: &str = "LUMINO1:";
/// 分享文本最大长度（字符数，含前缀）。
pub const MAX_LINE_LEN: usize = 4 * 1024 * 1024;
/// 解压后 JSON 最大字节数（防压缩炸弹）。
pub const MAX_DECODED_BYTES: usize = 8 * 1024 * 1024;
/// zstd 压缩级别。
const ZSTD_LEVEL: i32 = 3;

#[derive(Debug, Error)]
pub enum ShareError {
    /// 空文本
    #[error("分享文本为空")]
    Empty,
    /// 文本超长
    #[error("分享文本超过 {MAX_LINE_LEN} 字符")]
    TooLong,
    /// 前缀不是本格式
    #[error("不是 LUMINO 形状分享文本")]
    WrongPrefix,
    /// 版本不支持
    #[error("不支持的分享版本：{0}")]
    UnsupportedVersion(String),
    /// base64 解码失败
    #[error("分享文本 base64 损坏：{0}")]
    Base64(String),
    /// zstd 解压失败
    #[error("分享文本压缩数据损坏：{0}")]
    Compress(String),
    /// JSON 解析失败
    #[error("分享文本 JSON 损坏：{0}")]
    Json(String),
    /// 类型不支持（如 drawing）
    #[error("分享类型暂不支持：{0}")]
    UnsupportedKind(String),
    /// PPQ 非法
    #[error("PPQ 必须大于 0（实际 {0}）")]
    InvalidPpq(u16),
    /// 解压后超长
    #[error("分享文本解压后超过 {MAX_DECODED_BYTES} 字节")]
    DecodedTooLarge,
    /// 形状校验失败
    #[error("分享的形状非法：{0}")]
    InvalidShape(String),
    /// beats/tick 转换失败
    #[error("分享坐标转换失败：{0}")]
    Convert(String),
}

#[derive(Debug, Serialize, Deserialize)]
struct SharePayload {
    /// 创建者版本（仅诊断，不参与兼容判断）
    #[serde(default)]
    app: String,
    /// 载荷类型：当前只支持 `shapes`
    kind: String,
    /// 坐标单位：固定 `beats`
    unit: String,
    /// 形状列表（坐标已归一化为 beats）
    shapes: Vec<ShapeModel>,
}

/// 把形状编码成一行分享文本。
///
/// 编码前会逐个 `validate()`；返回的文本不含换行，可直接放剪贴板。
pub fn encode_shapes(shapes: &[ShapeModel], ppq: u16) -> Result<String, ShareError> {
    if ppq == 0 {
        return Err(ShareError::InvalidPpq(ppq));
    }
    for shape in shapes {
        shape
            .validate()
            .map_err(|error| ShareError::InvalidShape(error.to_string()))?;
    }

    let mut shapes = shapes.to_vec();
    for shape in &mut shapes {
        convert::to_beats(shape, ppq)?;
    }
    let payload = SharePayload {
        app: format!("lumino {}", env!("CARGO_PKG_VERSION")),
        kind: "shapes".to_string(),
        unit: "beats".to_string(),
        shapes,
    };
    let json = serde_json::to_vec(&payload)
        .map_err(|error| ShareError::Json(format!("序列化失败：{error}")))?;
    let compressed = zstd::stream::encode_all(Cursor::new(json), ZSTD_LEVEL)
        .map_err(|error| ShareError::Compress(error.to_string()))?;
    let encoded = STANDARD.encode(compressed);
    Ok(format!("{PREFIX}{encoded}"))
}

/// 把一行分享文本解码为目标 PPQ 的形状。
///
/// 文本中的空白/换行会被忽略（邮件、聊天软件折行）；解码后逐个 `validate()`。
pub fn decode_shapes(line: &str, target_ppq: u16) -> Result<Vec<ShapeModel>, ShareError> {
    if target_ppq == 0 {
        return Err(ShareError::InvalidPpq(target_ppq));
    }
    if line.trim().is_empty() {
        return Err(ShareError::Empty);
    }
    if line.chars().count() > MAX_LINE_LEN {
        return Err(ShareError::TooLong);
    }

    let compact: String = line.chars().filter(|ch| !ch.is_whitespace()).collect();
    let Some(rest) = compact.strip_prefix("LUMINO") else {
        return Err(ShareError::WrongPrefix);
    };
    let Some((version, encoded)) = rest.split_once(':') else {
        return Err(ShareError::WrongPrefix);
    };
    if version != "1" {
        return Err(ShareError::UnsupportedVersion(version.to_string()));
    }
    if encoded.is_empty() {
        return Err(ShareError::Empty);
    }

    let compressed = STANDARD
        .decode(encoded)
        .map_err(|error| ShareError::Base64(error.to_string()))?;
    let mut decoder = zstd::stream::read::Decoder::new(Cursor::new(compressed))
        .map_err(|error| ShareError::Compress(error.to_string()))?;
    let mut json = Vec::new();
    decoder
        .by_ref()
        .take(MAX_DECODED_BYTES as u64 + 1)
        .read_to_end(&mut json)
        .map_err(|error| ShareError::Compress(error.to_string()))?;
    if json.len() > MAX_DECODED_BYTES {
        return Err(ShareError::DecodedTooLarge);
    }

    let payload: SharePayload =
        serde_json::from_slice(&json).map_err(|error| ShareError::Json(error.to_string()))?;
    match payload.kind.as_str() {
        "shapes" => {}
        other => return Err(ShareError::UnsupportedKind(other.to_string())),
    }
    if payload.unit != "beats" {
        return Err(ShareError::Convert(format!(
            "未知坐标单位：{}",
            payload.unit
        )));
    }

    let mut shapes = payload.shapes;
    for shape in &mut shapes {
        convert::from_beats(shape, target_ppq)?;
        shape
            .validate()
            .map_err(|error| ShareError::InvalidShape(error.to_string()))?;
    }
    Ok(shapes)
}
