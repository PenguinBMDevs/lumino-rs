pub mod constants;
pub mod dms;
pub mod document;
pub mod error;
pub mod event;
pub mod info;
pub mod loader;
pub mod params;
pub mod track;

pub use dms::DmsInfo;
pub use document::MidiDocument;
pub use error::MidiError;
pub use event::MidiEvent;
pub use info::MidiInfo;
pub use track::{TrackManager, TrackView, TrackVisibility};

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
    pub document: Option<MidiDocument>,
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
