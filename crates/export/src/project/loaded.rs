//! 导入的外部数据缓存格式定义
//!
//! `data/loaded/` 目录下存储从外部导入文件的解析后数据快照。

use lumino_midi_loader::{DmsInfo, MidiInfo};

use crate::{ExportError, ExportResult};

/// 导入的 MIDI 数据缓存
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LoadedMidiData {
    /// 原始文件信息
    pub original_info: MidiInfo,
    /// 原始 MIDI 字节数据（保留原始数据以便导出）
    pub raw_midi_bytes: Vec<u8>,
    /// 解析后的文档（可选，懒加载）
    /// 如果已解析，存储 CompactEvent 扁平数组
    pub parsed_events: Option<Vec<u8>>,
    /// 解析后的音轨范围
    pub track_event_ranges: Option<Vec<(usize, usize)>>,
    /// 解析后的 tempo 变化
    pub tempo_changes: Option<Vec<(u32, f32)>>,
    /// 导入时间
    pub imported_at: String,
}

/// 导入的 DMS 数据缓存
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LoadedDmsData {
    /// 原始 DMS 信息
    pub original_info: DmsInfo,
    /// 轻量级 DMS 原始数据（解压后）
    pub raw_dms_data: Vec<u8>,
    /// 是否已转换为 MIDI
    pub converted_to_midi: bool,
    /// 转换后的 MIDI 字节（如果已转换）
    pub converted_midi_bytes: Option<Vec<u8>>,
    /// 导入时间
    pub imported_at: String,
}

/// 导入的 LMPJ 数据缓存
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LoadedLmpjData {
    /// 原始 LMPJ 信息
    pub original_info: MidiInfo,
    /// 原始 LMPJ 的 MidiInfo
    pub midi_info: MidiInfo,
    /// 原始 MIDI 字节数据
    pub midi_data: Vec<u8>,
    /// 导入时间
    pub imported_at: String,
}

/// 导入数据文件头（8 bytes）
#[derive(Debug, Clone, Copy)]
pub struct LmloadedHeader {
    /// 魔数: b"LMLD" (4 bytes)
    pub magic: [u8; 4],
    /// 数据格式版本: u16
    pub version: u16,
    /// 数据类型: u8 (0=midi, 1=dms, 2=lmpj)
    pub data_type: u8,
    /// 保留: u8
    pub _reserved: u8,
}

impl LmloadedHeader {
    /// 文件头大小
    pub const SIZE: usize = 8;

    /// 编码为字节数组
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0..4].copy_from_slice(&self.magic);
        buf[4..6].copy_from_slice(&self.version.to_le_bytes());
        buf[6] = self.data_type;
        buf[7] = self._reserved;
        buf
    }

    /// 从字节数组解码
    pub fn from_bytes(bytes: &[u8]) -> ExportResult<Self> {
        if bytes.len() < Self::SIZE {
            return Err(ExportError::FileFormat("lmloaded header: too short".into()));
        }
        let mut magic = [0u8; 4];
        magic.copy_from_slice(&bytes[0..4]);
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        let data_type = bytes[6];
        let _reserved = bytes[7];
        Ok(Self {
            magic,
            version,
            data_type,
            _reserved,
        })
    }
}

/// 导入数据类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum LoadedDataType {
    Midi = 0,
    Dms = 1,
    Lmpj = 2,
}

impl LoadedDataType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Midi),
            1 => Some(Self::Dms),
            2 => Some(Self::Lmpj),
            _ => None,
        }
    }
}

/// 通用导入数据编码
pub fn encode_loaded_data<T: serde::Serialize>(
    data: &T,
    data_type: LoadedDataType,
) -> ExportResult<Vec<u8>> {
    let mut result = Vec::new();

    let header = LmloadedHeader {
        magic: *b"LMLD",
        version: 1,
        data_type: data_type as u8,
        _reserved: 0,
    };
    result.extend_from_slice(&header.to_bytes());

    let serialized = bincode::serialize(data)
        .map_err(|e| ExportError::Encoding(format!("lmloaded bincode: {e}")))?;

    let compressed = zstd::stream::encode_all(std::io::Cursor::new(serialized), 3)
        .map_err(|e| ExportError::Compression(format!("lmloaded zstd: {e}")))?;

    result.extend_from_slice(&compressed);
    Ok(result)
}

/// 通用导入数据解码（返回解压后的 bincode 字节，由调用方反序列化）
pub fn decode_loaded_data(bytes: &[u8]) -> ExportResult<(LoadedDataType, Vec<u8>)> {
    if bytes.len() < LmloadedHeader::SIZE {
        return Err(ExportError::FileFormat("lmloaded: too short".into()));
    }

    let header = LmloadedHeader::from_bytes(bytes)?;
    if &header.magic != b"LMLD" {
        return Err(ExportError::FileFormat("lmloaded: invalid magic".into()));
    }
    if header.version != 1 {
        return Err(ExportError::FileFormat(format!(
            "lmloaded: unsupported version {}",
            header.version
        )));
    }

    let data_type = LoadedDataType::from_u8(header.data_type).ok_or_else(|| {
        ExportError::FileFormat(format!("lmloaded: unknown data type {}", header.data_type))
    })?;

    let decompressed =
        zstd::stream::decode_all(std::io::Cursor::new(&bytes[LmloadedHeader::SIZE..]))
            .map_err(|e| ExportError::Compression(format!("lmloaded decompression: {e}")))?;

    Ok((data_type, decompressed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lmloaded_header_roundtrip() {
        let header = LmloadedHeader {
            magic: *b"LMLD",
            version: 1,
            data_type: 0,
            _reserved: 0,
        };
        let bytes = header.to_bytes();
        let decoded = LmloadedHeader::from_bytes(&bytes).expect("解码LmloadedHeader失败");
        assert_eq!(&decoded.magic, b"LMLD");
        assert_eq!(decoded.version, 1);
        assert_eq!(decoded.data_type, 0);
    }

    #[test]
    fn test_encode_decode_midi_data() {
        let data = LoadedMidiData {
            original_info: MidiInfo {
                path: std::path::PathBuf::from("test.mid"),
                track_count: 2,
                total_notes: 100,
                duration_ticks: 960,
                division: 480,
                parse_progress: None,
            },
            raw_midi_bytes: vec![0x4D, 0x54, 0x68, 0x64],
            parsed_events: None,
            track_event_ranges: None,
            tempo_changes: Some(vec![(0, 120.0)]),
            imported_at: "2026-05-28T10:00:00+08:00".into(),
        };

        let encoded = encode_loaded_data(&data, LoadedDataType::Midi).expect("编码已加载的MIDI数据失败");
        let (dtype, decoded_bytes) = decode_loaded_data(&encoded).expect("解码已加载的数据失败");

        assert_eq!(dtype, LoadedDataType::Midi);
        let decoded: LoadedMidiData = bincode::deserialize(&decoded_bytes).expect("反序列化LoadedMidiData失败");
        assert_eq!(decoded.original_info.track_count, 2);
        assert_eq!(decoded.raw_midi_bytes.len(), 4);
    }
}
