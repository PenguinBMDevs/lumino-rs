//! 工程专用数据文件格式定义
//!
//! .lmtemp / .lmsig / .lmctl / .lmnames 均使用专用魔数 + bincode + zstd。

use super::folder::{decode_binary_file, encode_binary_file};
use lumino_core::error::Result;

/// 全局速度变化数据（.lmtemp）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LmtempData {
    /// 速度变化列表: (tick, bpm)
    pub tempo_changes: Vec<(u32, f32)>,
    /// 默认 BPM（如果列表为空则使用此值）
    pub default_bpm: f32,
}

impl LmtempData {
    pub const MAGIC: &[u8; 4] = b"LMTM";

    pub fn encode(&self) -> Result<Vec<u8>> {
        encode_binary_file(Self::MAGIC, 1, self)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        decode_binary_file(bytes, Self::MAGIC)
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
    pub const MAGIC: &[u8; 4] = b"LMSG";

    pub fn encode(&self) -> Result<Vec<u8>> {
        encode_binary_file(Self::MAGIC, 1, self)
    }

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
    pub const MAGIC: &[u8; 4] = b"LMCT";

    pub fn encode(&self) -> Result<Vec<u8>> {
        encode_binary_file(Self::MAGIC, 1, self)
    }

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
    pub const MAGIC: &[u8; 4] = b"LMTX";

    pub fn encode(&self) -> Result<Vec<u8>> {
        encode_binary_file(Self::MAGIC, 1, self)
    }

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
    pub const MAGIC: &[u8; 4] = b"LMSY";

    pub fn encode(&self) -> Result<Vec<u8>> {
        encode_binary_file(Self::MAGIC, 1, self)
    }

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
    pub const MAGIC: &[u8; 4] = b"LMNM";

    pub fn encode(&self) -> Result<Vec<u8>> {
        encode_binary_file(Self::MAGIC, 1, self)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        decode_binary_file(bytes, Self::MAGIC)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lmtemp_roundtrip() {
        let data = LmtempData {
            tempo_changes: vec![(0, 120.0), (960, 140.0)],
            default_bpm: 120.0,
        };
        let encoded = data.encode().expect("编码LmtempData失败");
        assert_eq!(&encoded[0..4], LmtempData::MAGIC);
        let decoded = LmtempData::decode(&encoded).expect("解码LmtempData失败");
        assert_eq!(decoded.tempo_changes.len(), 2);
        assert!((decoded.default_bpm - 120.0).abs() < 0.001);
    }

    #[test]
    fn test_lmsig_roundtrip() {
        let data = LmsigData {
            time_signatures: vec![(0, 4, 4), (960, 3, 4)],
            key_signatures: vec![(0, 0, true)],
        };
        let encoded = data.encode().expect("编码LmsigData失败");
        assert_eq!(&encoded[0..4], LmsigData::MAGIC);
        let decoded = LmsigData::decode(&encoded).expect("解码LmsigData失败");
        assert_eq!(decoded.time_signatures.len(), 2);
    }

    #[test]
    fn test_lmctl_roundtrip() {
        let data = LmctlData {
            control_changes: vec![(0, 0, 0, 7, 100)],
            program_changes: vec![(0, 0, 0, 1)],
            pitch_bends: vec![(480, 0, 0, 8192)],
        };
        let encoded = data.encode().expect("编码LmctlData失败");
        assert_eq!(&encoded[0..4], LmctlData::MAGIC);
        let decoded = LmctlData::decode(&encoded).expect("解码LmctlData失败");
        assert_eq!(decoded.control_changes.len(), 1);
        assert_eq!(decoded.pitch_bends.len(), 1);
    }

    #[test]
    fn test_lmtxt_roundtrip() {
        let data = LmtxtData {
            lyrics: vec![(0, 0, b"la".to_vec()), (480, 0, b"la".to_vec())],
            markers: vec![(960, 0, b"Chorus".to_vec())],
        };
        let encoded = data.encode().expect("编码LmtxtData失败");
        assert_eq!(&encoded[0..4], LmtxtData::MAGIC);
        let decoded = LmtxtData::decode(&encoded).expect("解码LmtxtData失败");
        assert_eq!(decoded.lyrics.len(), 2);
        assert_eq!(decoded.markers.len(), 1);
        assert_eq!(decoded.lyrics[0].2, b"la");
    }

    #[test]
    fn test_lmsyx_roundtrip() {
        let data = LmsyxData {
            sys_ex: vec![(0, 0, b"\x01\x02\x03\xF7".to_vec())],
        };
        let encoded = data.encode().expect("编码LmsyxData失败");
        assert_eq!(&encoded[0..4], LmsyxData::MAGIC);
        let decoded = LmsyxData::decode(&encoded).expect("解码LmsyxData失败");
        assert_eq!(decoded.sys_ex.len(), 1);
        assert_eq!(decoded.sys_ex[0].2, b"\x01\x02\x03\xF7");
    }

    #[test]
    fn test_lmnames_roundtrip() {
        let data = LmnamesData {
            track_names: vec![Some("Piano".into()), Some("Bass".into()), None],
        };
        let encoded = data.encode().expect("编码LmnamesData失败");
        assert_eq!(&encoded[0..4], LmnamesData::MAGIC);
        let decoded = LmnamesData::decode(&encoded).expect("解码LmnamesData失败");
        assert_eq!(decoded.track_names.len(), 3);
        assert_eq!(decoded.track_names[0], Some("Piano".into()));
        assert_eq!(decoded.track_names[2], None);
    }

    #[test]
    fn test_invalid_magic() {
        let mut bytes = vec![0u8; 20];
        bytes[0..4].copy_from_slice(b"XXXX");
        assert!(LmtempData::decode(&bytes).is_err());
        assert!(LmsigData::decode(&bytes).is_err());
        assert!(LmctlData::decode(&bytes).is_err());
        assert!(LmtxtData::decode(&bytes).is_err());
        assert!(LmsyxData::decode(&bytes).is_err());
        assert!(LmnamesData::decode(&bytes).is_err());
    }
}
