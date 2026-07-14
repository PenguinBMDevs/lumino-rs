//! 音轨数据文件格式定义与编解码
//!
//! `.lmtrack` 文件存储单个音轨的解析后事件数据，采用 bincode + zstd 压缩。

use lumino_midi_model::compact::CompactEvent;

use crate::{ExportError, ExportResult};

/// 音轨可见性（序列化版本）
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum TrackVisibilitySer {
    /// 可见
    Visible = 0,
    /// 静音
    Muted = 1,
    /// 隐藏
    Hidden = 2,
}

/// 音轨元数据
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrackMeta {
    /// 音轨编号
    pub track_id: u16,
    /// 音轨名称
    pub name: String,
    /// MIDI 通道 (0-15)
    pub channel: u8,
    /// 端口 (0-15)
    pub port: u8,
    /// 可见性
    pub visibility: TrackVisibilitySer,
    /// Solo 状态
    pub solo: bool,
    /// 是否为鼓音轨
    pub is_drum: bool,
    /// 总 tick 范围（此音轨最后一个事件的 tick）
    pub max_tick: u32,
}

/// 音轨数据文件头（8 bytes）
#[derive(Debug, Clone, Copy)]
pub struct LmtrackHeader {
    /// 魔数: b"LMTR" (4 bytes)
    pub magic: [u8; 4],
    /// 音轨数据格式版本: u16
    pub version: u16,
    /// 音轨编号: u16
    pub track_id: u16,
}

impl LmtrackHeader {
    /// 文件头大小
    pub const SIZE: usize = 8;

    /// 编码为字节数组
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0..4].copy_from_slice(&self.magic);
        buf[4..6].copy_from_slice(&self.version.to_le_bytes());
        buf[6..8].copy_from_slice(&self.track_id.to_le_bytes());
        buf
    }

    /// 从字节数组解码
    pub fn from_bytes(bytes: &[u8]) -> ExportResult<Self> {
        if bytes.len() < Self::SIZE {
            return Err(ExportError::FileFormat("lmtrack header: too short".into()));
        }
        let mut magic = [0u8; 4];
        magic.copy_from_slice(&bytes[0..4]);
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        let track_id = u16::from_le_bytes([bytes[6], bytes[7]]);
        Ok(Self {
            magic,
            version,
            track_id,
        })
    }
}

/// 音轨事件存储结构（序列化主体）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LmtrackData {
    /// 音轨元数据
    pub meta: TrackMeta,
    /// 事件数据（CompactEvent 数组的原始字节）
    /// 注意：事件已按 tick 排序，直接写入无需再处理
    pub events: Vec<u8>,
    /// 事件数量（用于解码时验证）
    pub event_count: u64,
    /// 此音轨的音符数量（NoteOn 事件数）
    pub note_count: u64,
}

impl LmtrackData {
    /// 从 CompactEvent 切片创建
    pub fn from_compact_events(meta: TrackMeta, events: &[CompactEvent]) -> Self {
        let note_count = events
            .iter()
            .filter(|e| matches!(e.kind(), lumino_midi_model::compact::EventKind::NoteOn))
            .count() as u64;

        // CompactEvent 扁平化为字节数组
        let mut event_bytes = Vec::with_capacity(events.len() * 12);
        for ev in events {
            event_bytes.extend_from_slice(ev.as_bytes());
        }

        Self {
            meta,
            events: event_bytes,
            event_count: events.len() as u64,
            note_count,
        }
    }

    /// 获取 CompactEvent 迭代器（零拷贝视图）
    pub fn compact_events(&self) -> impl Iterator<Item = CompactEvent> + '_ {
        self.events.chunks_exact(12).map(|chunk| {
            let bytes: &[u8; 12] = chunk.try_into().unwrap_or(&[0; 12]);
            CompactEvent::from_bytes(bytes)
        })
    }

    /// 编码为字节（文件头 + bincode + zstd）
    pub fn encode(&self) -> ExportResult<Vec<u8>> {
        let mut result = Vec::new();

        // 写入文件头
        let header = LmtrackHeader {
            magic: *b"LMTR",
            version: 1,
            track_id: self.meta.track_id,
        };
        result.extend_from_slice(&header.to_bytes());

        // bincode 序列化主体
        let serialized = bincode::serialize(self)
            .map_err(|e| ExportError::Encoding(format!("lmtrack bincode: {e}")))?;

        // zstd 压缩
        let compressed = zstd::stream::encode_all(std::io::Cursor::new(serialized), 3)
            .map_err(|e| ExportError::Compression(format!("lmtrack zstd: {e}")))?;

        result.extend_from_slice(&compressed);
        Ok(result)
    }

    /// 从字节解码
    pub fn decode(bytes: &[u8]) -> ExportResult<Self> {
        if bytes.len() < LmtrackHeader::SIZE {
            return Err(ExportError::FileFormat("lmtrack: too short".into()));
        }

        // 验证魔数
        let header = LmtrackHeader::from_bytes(bytes)?;
        if &header.magic != b"LMTR" {
            return Err(ExportError::FileFormat("lmtrack: invalid magic".into()));
        }
        if header.version != 1 {
            return Err(ExportError::FileFormat(format!(
                "lmtrack: unsupported version {}",
                header.version
            )));
        }

        // zstd 解压
        let decompressed =
            zstd::stream::decode_all(std::io::Cursor::new(&bytes[LmtrackHeader::SIZE..]))
                .map_err(|e| ExportError::Compression(format!("lmtrack decompression: {e}")))?;

        // bincode 反序列化
        bincode::deserialize(&decompressed)
            .map_err(|e| ExportError::Encoding(format!("lmtrack decode: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_meta(track_id: u16) -> TrackMeta {
        TrackMeta {
            track_id,
            name: format!("Track {}", track_id),
            channel: 0,
            port: 0,
            visibility: TrackVisibilitySer::Visible,
            solo: false,
            is_drum: false,
            max_tick: 1000,
        }
    }

    #[test]
    fn test_lmtrack_header_roundtrip() {
        let header = LmtrackHeader {
            magic: *b"LMTR",
            version: 1,
            track_id: 42,
        };
        let bytes = header.to_bytes();
        let decoded = LmtrackHeader::from_bytes(&bytes).expect("解码LmtrackHeader失败");
        assert_eq!(&decoded.magic, b"LMTR");
        assert_eq!(decoded.version, 1);
        assert_eq!(decoded.track_id, 42);
    }

    #[test]
    fn test_lmtrack_encode_decode() {
        let meta = create_test_meta(0);
        let events = vec![
            CompactEvent::new(0, 0, lumino_midi_model::compact::EventKind::NoteOn, 0, 60, 100),
            CompactEvent::new(
                480,
                0,
                lumino_midi_model::compact::EventKind::NoteOff,
                0,
                60,
                0,
            ),
        ];
        let data = LmtrackData::from_compact_events(meta, &events);

        assert_eq!(data.event_count, 2);
        assert_eq!(data.note_count, 1);

        let encoded = data.encode().expect("编码LmtrackData失败");
        let decoded = LmtrackData::decode(&encoded).expect("解码LmtrackData失败");

        assert_eq!(decoded.meta.track_id, 0);
        assert_eq!(decoded.event_count, 2);
        assert_eq!(decoded.note_count, 1);
    }

    #[test]
    fn test_lmtrack_invalid_magic() {
        let mut bytes = vec![0u8; 20];
        bytes[0..4].copy_from_slice(b"XXXX");
        let result = LmtrackData::decode(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_compact_events_iterator() {
        let meta = create_test_meta(1);
        let events = vec![
            CompactEvent::new(
                100,
                1,
                lumino_midi_model::compact::EventKind::NoteOn,
                2,
                64,
                80,
            ),
            CompactEvent::new(
                200,
                1,
                lumino_midi_model::compact::EventKind::NoteOff,
                2,
                64,
                0,
            ),
        ];
        let data = LmtrackData::from_compact_events(meta, &events);

        let collected: Vec<_> = data.compact_events().collect();
        assert_eq!(collected.len(), 2);
        assert_eq!(collected[0].delta_tick(), 100);
        assert_eq!(collected[1].delta_tick(), 200);
    }
}
