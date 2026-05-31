pub mod constants;
pub mod dms;
pub mod document;
pub mod error;
pub mod event;
pub mod info;
pub mod loader;
pub mod quantize;
pub mod track;

pub use dms::DmsInfo;
pub use document::MidiDocument;
pub use error::MidiError;
pub use event::MidiEvent;
pub use info::MidiInfo;
pub use midly::loader::PackedControlEvent;
pub use track::{TrackManager, TrackView, TrackVisibility};

/// 将 BPM 转换为微秒每拍（tempo）
#[inline]
pub fn bpm_to_tempo(bpm: f64) -> u32 {
    (60_000_000.0 / bpm).round() as u32
}

/// 将微秒每拍（tempo）转换为 BPM
#[inline]
pub fn tempo_to_bpm(tempo: u32) -> f64 {
    60_000_000.0 / tempo as f64
}

use std::sync::Arc;

/// LMPJ 文件数据结构（用于序列化/反序列化）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LmpjData {
    pub info: MidiInfo,
    pub midi_data: Option<Vec<u8>>,
}

impl LmpjData {
    pub fn from_parsed_midi(parsed: &ParsedMidi) -> Self {
        Self {
            info: parsed.info.clone(),
            midi_data: parsed.midi_data.clone(),
        }
    }

    pub fn to_parsed_midi(self) -> ParsedMidi {
        ParsedMidi {
            info: self.info,
            midi_data: self.midi_data,
            document: None,
        }
    }
}

/// 解析后的MIDI数据
#[derive(Debug, Clone)]
pub struct ParsedMidi {
    pub info: MidiInfo,
    /// 原始 MIDI 字节（LMPJ 文件使用）
    pub midi_data: Option<Vec<u8>>,
    /// 紧凑内存解析结果（常规 MIDI 文件使用）
    ///
    /// 使用 `Arc<MidiDocument>` 而非裸 `MidiDocument`，使得 UI 可以通过 Arc::clone
    /// 共享同一份事件数据而无需深拷贝 `document.clone()`（后者会复制 `Vec<CompactEvent>`，
    /// 对 10M 事件的黑乐谱来说就是额外 120MB）。
    pub document: Option<Arc<MidiDocument>>,
}

impl serde::Serialize for ParsedMidi {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(serde::Serialize)]
        struct Helper<'a> {
            info: &'a crate::MidiInfo,
            midi_data: &'a Option<Vec<u8>>,
        }
        Helper {
            info: &self.info,
            midi_data: &self.midi_data,
        }
        .serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for ParsedMidi {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Helper {
            info: crate::MidiInfo,
            midi_data: Option<Vec<u8>>,
        }
        let h = Helper::deserialize(deserializer)?;
        Ok(ParsedMidi {
            info: h.info,
            midi_data: h.midi_data,
            document: None,
        })
    }
}

impl ParsedMidi {
    pub fn take_midi_data(&mut self) -> Option<Vec<u8>> {
        self.midi_data.take()
    }

    /// 获取 MIDI 原始字节数据（用于音频导出等场景，避免重复读盘）
    ///
    /// 优先返回 `midi_data`，如果为 None 则回退到从 `info.path` 读取文件。
    pub fn get_midi_bytes(&self) -> crate::Result<Vec<u8>> {
        if let Some(ref bytes) = self.midi_data {
            return Ok(bytes.clone());
        }
        if !self.info.path.as_os_str().is_empty() {
            return std::fs::read(&self.info.path)
                .map_err(|e| crate::CoreError::Io(std::io::Error::other(e)));
        }
        Err(crate::CoreError::InvalidArgument(
            "ParsedMidi 中既无 midi_data 也无 info.path".to_string(),
        ))
    }
}

/// 解析后的 DMS 数据（轻量级）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ParsedDms {
    pub info: DmsInfo,
    #[serde(skip)]
    data: Option<lumino_dms::DmsLightweightData>,
}

impl ParsedDms {
    pub fn parse_full(&self) -> Result<lumino_dms::DmsCompositeNode, String> {
        self.data
            .as_ref()
            .ok_or_else(|| "需要加载完整DMS数据才能解析".to_string())?
            .parse_full()
            .map_err(|e| format!("解析 DMS 节点树失败: {e}"))
    }

    pub fn data_size(&self) -> usize {
        self.data.as_ref().map(|d| d.len()).unwrap_or(0)
    }
}
