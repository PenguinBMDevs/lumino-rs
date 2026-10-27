//! 工程专用数据文件格式定义
//!
//! .lmtemp / .lmsig / .lmctl / .lmnames / .lmshapes 均使用专用魔数 + bincode + zstd。

use super::folder::{decode_binary_file, decode_binary_file_versioned, encode_binary_file};
use lumino_core::error::{CoreError, Result};
use lumino_core::shape::ShapeModel;

mod v1;
mod v2;
use v1::LmshapesDataV1;
use v2::LmshapesDataV2;

/// `.lmshapes` 当前编码版本
///
/// - v2：`ShapeStyle` 追加 `align / ends / union / apart / borders / areas`（DRAW-040）。
/// - v3：`ShapeAnchor` 追加 `bend`（DRAW-042）。
const LMSHAPES_VERSION: u16 = 3;

/// 全局速度变化数据（.lmtemp）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LmtempData {
    /// 速度变化列表: (tick, bpm)
    pub tempo_changes: Vec<(u32, f32)>,
    /// 默认 BPM（如果列表为空则使用此值）
    pub default_bpm: f32,
}

impl LmtempData {
    /// 文件魔数
    pub const MAGIC: &[u8; 4] = b"LMTM";

    /// 编码为二进制文件字节
    pub fn encode(&self) -> Result<Vec<u8>> {
        encode_binary_file(Self::MAGIC, 1, self)
    }

    /// 从二进制字节解码
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        decode_binary_file(bytes, Self::MAGIC)
    }
}

/// 形状数据（.lmshapes）
///
/// 形状是**文档对象**：随工程保存、参数可回改、可与它生成的音符同组撤销。
/// 独立成文件遵循「新部件缺失即空」约定：老工程无此文件即空形状列表，
/// 无需提升 `format_version`；形状为空时保存侧**不写**该部件，老工程保存后字节不变。
///
/// ⚠️ 落盘纪律（DRAW-040 / D1）：
/// - bincode 是位置编码：`ShapeModel` / `ShapeStyle` **增删字段或重排**必须提升
///   本部件版本（当前 [`LMSHAPES_VERSION`]），不得静默改结构；
/// - `ShapeOp` / `ShapeKind` 的**变体只能追加到末尾**，追加变体**不需要**提版本
///   （旧数据不会出现新变体索引；重排或删除才会让旧工程解错语义）；
/// - `encode` 恒写 v3；`decode` 接受 v1 / v2 / v3。
///
/// 旧版本升级的字节变化是**预期行为**：含形状部件的工程重新保存后版本变为 3；
/// **无**形状部件的工程仍然不写该部件，保存后字节不变（见 `save/tests.rs`）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LmshapesData {
    /// 形状列表（顺序即 Shapes 列表顺序）
    pub shapes: Vec<ShapeModel>,
}

impl LmshapesData {
    /// 文件魔数
    pub const MAGIC: &[u8; 4] = b"LMSH";

    /// 编码为二进制文件字节（恒写 [`LMSHAPES_VERSION`]）
    pub fn encode(&self) -> Result<Vec<u8>> {
        encode_binary_file(Self::MAGIC, LMSHAPES_VERSION, self)
    }

    /// 从二进制字节解码（接受 v1 / v2 / v3）
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        decode_binary_file_versioned(
            bytes,
            Self::MAGIC,
            &[1, 2, LMSHAPES_VERSION],
            |version, payload| match version {
                1 => {
                    let v1: LmshapesDataV1 =
                        bincode::deserialize(payload).map_err(CoreError::from)?;
                    Ok(v1.into())
                }
                2 => {
                    let v2: LmshapesDataV2 =
                        bincode::deserialize(payload).map_err(CoreError::from)?;
                    Ok(v2.into())
                }
                LMSHAPES_VERSION => bincode::deserialize(payload).map_err(CoreError::from),
                other => Err(CoreError::FileFormat(format!(
                    "lmshapes: unsupported version {other}"
                ))),
            },
        )
    }
}

/// 拍号/调号数据（.lmsig）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LmsigData {
    /// 拍号变化: (tick, numerator, denominator)
    pub time_signatures: Vec<(u32, u8, u8)>,
    /// 调号变化: (tick, key, is_major)
    pub key_signatures: Vec<(u32, i8, bool)>,
}

impl LmsigData {
    /// 文件魔数
    pub const MAGIC: &[u8; 4] = b"LMSG";

    /// 编码为二进制文件字节
    pub fn encode(&self) -> Result<Vec<u8>> {
        encode_binary_file(Self::MAGIC, 1, self)
    }

    /// 从二进制字节解码
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        decode_binary_file(bytes, Self::MAGIC)
    }
}

/// 控制事件数据（.lmctl）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LmctlData {
    /// 控制变更事件: (tick, track_id, channel, controller, value)
    pub control_changes: Vec<(u32, u16, u8, u8, u8)>,
    /// 程序变更事件: (tick, track_id, channel, program)
    pub program_changes: Vec<(u32, u16, u8, u8)>,
    /// 弯音事件: (tick, track_id, channel, value)
    pub pitch_bends: Vec<(u32, u16, u8, i16)>,
}

impl LmctlData {
    /// 文件魔数
    pub const MAGIC: &[u8; 4] = b"LMCT";

    /// 编码为二进制文件字节
    pub fn encode(&self) -> Result<Vec<u8>> {
        encode_binary_file(Self::MAGIC, 1, self)
    }

    /// 从二进制字节解码
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        decode_binary_file(bytes, Self::MAGIC)
    }
}

/// 文本 meta 事件数据（.lmtxt）
///
/// 包含歌词与标记。文本 payload 以原始字节保存，避免在工程格式层强制指定编码；
/// 渲染/导出时按 Lumino 的 MIDI 文本解码规则（UTF-8 → Shift-JIS → GBK → Latin-1）处理。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LmtxtData {
    /// 歌词事件: (tick, track_id, text bytes)
    pub lyrics: Vec<(u32, u16, Vec<u8>)>,
    /// 标记事件: (tick, track_id, text bytes)
    pub markers: Vec<(u32, u16, Vec<u8>)>,
}

impl LmtxtData {
    /// 文件魔数
    pub const MAGIC: &[u8; 4] = b"LMTX";

    /// 编码为二进制文件字节
    pub fn encode(&self) -> Result<Vec<u8>> {
        encode_binary_file(Self::MAGIC, 1, self)
    }

    /// 从二进制字节解码
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        decode_binary_file(bytes, Self::MAGIC)
    }
}

/// 文本类 meta 事件数据（.lmmtx）
///
/// 与 `.lmtxt`（歌词/标记）分离成独立文件：`.lmtxt` 已是 v1 格式，
/// bincode 位置编码下加字段会破坏老工程解码；独立文件缺失即空
/// （加载侧 `exists()` 守卫），老工程零风险。
/// 文本 payload 以原始字节保存，meta_type 区分 0x01/0x02/0x04/0x07/0x08/0x09。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LmtextmetaData {
    /// 文本类事件: (tick, track_id, meta_type, text bytes)
    pub text_events: Vec<(u32, u16, u8, Vec<u8>)>,
}

impl LmtextmetaData {
    /// 文件魔数
    pub const MAGIC: &[u8; 4] = b"LMMT";

    /// 编码为二进制文件字节
    pub fn encode(&self) -> Result<Vec<u8>> {
        encode_binary_file(Self::MAGIC, 1, self)
    }

    /// 从二进制字节解码
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        decode_binary_file(bytes, Self::MAGIC)
    }
}

/// 触后事件数据（.lmcat）
///
/// 与 `.lmctl`（CC/PC/PB）分离成独立文件：`.lmctl` 已是 v1 格式，
/// bincode 位置编码下加字段会破坏老工程解码；独立文件缺失即空
/// （加载侧 `exists()` 守卫），老工程零风险。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LmcatData {
    /// 通道触后事件: (tick, track_id, channel, velocity)
    pub channel_aftertouch: Vec<(u32, u16, u8, u8)>,
    /// 复音触后事件: (tick, track_id, channel, key, velocity)
    pub poly_aftertouch: Vec<(u32, u16, u8, u8, u8)>,
}

impl LmcatData {
    /// 文件魔数
    pub const MAGIC: &[u8; 4] = b"LMAT";

    /// 编码为二进制文件字节
    pub fn encode(&self) -> Result<Vec<u8>> {
        encode_binary_file(Self::MAGIC, 1, self)
    }

    /// 从二进制字节解码
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        decode_binary_file(bytes, Self::MAGIC)
    }
}

/// SysEx 事件数据（.lmsyx）
///
/// SysEx 可能很大，因此单独成文件，避免与小型控制事件混排导致加载时被迫全部读入内存。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LmsyxData {
    /// SysEx 事件: (tick, track_id, data bytes)
    ///
    /// 注意：保存时不包含 0xF0 前缀，与 midly 的 `SysEx` payload 保持一致。
    pub sys_ex: Vec<(u32, u16, Vec<u8>)>,
}

impl LmsyxData {
    /// 文件魔数
    pub const MAGIC: &[u8; 4] = b"LMSY";

    /// 编码为二进制文件字节
    pub fn encode(&self) -> Result<Vec<u8>> {
        encode_binary_file(Self::MAGIC, 1, self)
    }

    /// 从二进制字节解码
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        decode_binary_file(bytes, Self::MAGIC)
    }
}

/// 音轨名称映射表（.lmnames）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LmnamesData {
    /// 音轨名称列表: index = track_id
    pub track_names: Vec<Option<String>>,
}

impl LmnamesData {
    /// 文件魔数
    pub const MAGIC: &[u8; 4] = b"LMNM";

    /// 编码为二进制文件字节
    pub fn encode(&self) -> Result<Vec<u8>> {
        encode_binary_file(Self::MAGIC, 1, self)
    }

    /// 从二进制字节解码
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        decode_binary_file(bytes, Self::MAGIC)
    }
}

#[cfg(test)]
mod tests;
