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
            // pitch_bends 为相对中心的偏移量（i16；0 = 中心），不是 raw 14-bit。
            pitch_bends: vec![(480, 0, 0, 0)],
        };
        let encoded = data.encode().expect("编码LmctlData失败");
        assert_eq!(&encoded[0..4], LmctlData::MAGIC);
        let decoded = LmctlData::decode(&encoded).expect("解码LmctlData失败");
        assert_eq!(decoded.control_changes.len(), 1);
        assert_eq!(decoded.pitch_bends.len(), 1);
        assert_eq!(decoded.pitch_bends[0].3, 0, "偏移量中心应为 0");
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
    fn test_lmtextmeta_roundtrip() {
        let data = LmtextmetaData {
            text_events: vec![
                (0, 0, 0x01, b"hello".to_vec()),
                (120, 1, 0x02, b"(c) test".to_vec()),
                (480, 0, 0x07, b"cue1".to_vec()),
            ],
        };
        let encoded = data.encode().expect("编码LmtextmetaData失败");
        assert_eq!(&encoded[0..4], LmtextmetaData::MAGIC);
        let decoded = LmtextmetaData::decode(&encoded).expect("解码LmtextmetaData失败");
        assert_eq!(decoded.text_events.len(), 3);
        assert_eq!(decoded.text_events[1].2, 0x02);
        assert_eq!(decoded.text_events[2].3, b"cue1");
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
    fn test_lmcat_roundtrip() {
        let data = LmcatData {
            channel_aftertouch: vec![(0, 0, 5, 64), (120, 1, 0, 100)],
            poly_aftertouch: vec![(240, 0, 3, 60, 80)],
        };
        let encoded = data.encode().expect("编码LmcatData失败");
        assert_eq!(&encoded[0..4], LmcatData::MAGIC);
        let decoded = LmcatData::decode(&encoded).expect("解码LmcatData失败");
        assert_eq!(decoded.channel_aftertouch.len(), 2);
        assert_eq!(decoded.poly_aftertouch, vec![(240, 0, 3, 60, 80)]);
        assert_eq!(decoded.channel_aftertouch[1].3, 100);
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
        assert!(LmcatData::decode(&bytes).is_err());
    }
}
