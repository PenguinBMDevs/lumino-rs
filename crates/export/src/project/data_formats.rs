//! 工程专用数据文件格式定义
//!
//! .lmtemp / .lmsig / .lmctl / .lmnames 均使用专用魔数 + bincode + zstd。

use crate::ExportResult;
use crate::project::folder::{decode_binary_file, encode_binary_file};

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

    pub fn encode(&self) -> ExportResult<Vec<u8>> {
        encode_binary_file(Self::MAGIC, 1, self)
    }

    pub fn decode(bytes: &[u8]) -> ExportResult<Self> {
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

    pub fn encode(&self) -> ExportResult<Vec<u8>> {
        encode_binary_file(Self::MAGIC, 1, self)
    }

    pub fn decode(bytes: &[u8]) -> ExportResult<Self> {
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

    pub fn encode(&self) -> ExportResult<Vec<u8>> {
        encode_binary_file(Self::MAGIC, 1, self)
    }

    pub fn decode(bytes: &[u8]) -> ExportResult<Self> {
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

    pub fn encode(&self) -> ExportResult<Vec<u8>> {
        encode_binary_file(Self::MAGIC, 1, self)
    }

    pub fn decode(bytes: &[u8]) -> ExportResult<Self> {
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
        let encoded = data.encode().unwrap();
        assert_eq!(&encoded[0..4], LmtempData::MAGIC);
        let decoded = LmtempData::decode(&encoded).unwrap();
        assert_eq!(decoded.tempo_changes.len(), 2);
        assert!((decoded.default_bpm - 120.0).abs() < 0.001);
    }

    #[test]
    fn test_lmsig_roundtrip() {
        let data = LmsigData {
            time_signatures: vec![(0, 4, 4), (960, 3, 4)],
            key_signatures: vec![(0, 0, true)],
        };
        let encoded = data.encode().unwrap();
        assert_eq!(&encoded[0..4], LmsigData::MAGIC);
        let decoded = LmsigData::decode(&encoded).unwrap();
        assert_eq!(decoded.time_signatures.len(), 2);
    }

    #[test]
    fn test_lmctl_roundtrip() {
        let data = LmctlData {
            control_changes: vec![(0, 0, 0, 7, 100)],
            program_changes: vec![(0, 0, 0, 1)],
            pitch_bends: vec![(480, 0, 0, 8192)],
        };
        let encoded = data.encode().unwrap();
        assert_eq!(&encoded[0..4], LmctlData::MAGIC);
        let decoded = LmctlData::decode(&encoded).unwrap();
        assert_eq!(decoded.control_changes.len(), 1);
        assert_eq!(decoded.pitch_bends.len(), 1);
    }

    #[test]
    fn test_lmnames_roundtrip() {
        let data = LmnamesData {
            track_names: vec![Some("Piano".into()), Some("Bass".into()), None],
        };
        let encoded = data.encode().unwrap();
        assert_eq!(&encoded[0..4], LmnamesData::MAGIC);
        let decoded = LmnamesData::decode(&encoded).unwrap();
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
        assert!(LmnamesData::decode(&bytes).is_err());
    }
}
