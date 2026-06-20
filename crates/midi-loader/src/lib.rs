pub mod constants;
pub mod dms;
pub mod document;
#[cfg(test)]
pub mod document_tests;
pub mod error;
pub mod event;
pub mod info;
pub mod loader;
pub mod note_info;
pub mod quantize;
pub mod track;

pub use dms::DmsInfo;
pub use document::MidiDocument;
pub use error::{LoaderError, LoaderResult};
pub use event::MidiEvent;
pub use info::MidiInfo;
pub use note_info::NoteInfo;
pub use track::{TrackManager, TrackView, TrackVisibility};

use std::sync::Arc;

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

/// LMPJ 文件数据结构（用于序列化/反序列化）
///
/// LMPJ 是 Lumino 原生工程格式。`midi_data` 存储从内存中 `MidiDocument` 重建的
/// MIDI 字节（含用户编辑的 tempo 等变动），加载时从中重建 `MidiDocument`。
///
/// **关键原则**：LMPJ 保存/加载完全不依赖原始 .mid 文件——工程数据自包含。
/// 保存时重建 midi_data 从内存 document，加载时从 midi_data 重建 document。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LmpjData {
    pub info: MidiInfo,
    /// 从内存 MidiDocument 重建的 MIDI 字节（含用户编辑）。
    /// 保存时由 lumino-export 从 document 导出，加载时由 loader 重建 document。
    pub midi_data: Option<Vec<u8>>,
}

impl LmpjData {
    /// 转换为 ParsedMidi，不保留 midi_data。
    ///
    /// 调用方应在加载 LMPJ 文件后调用此方法，然后使用 `midi_data` 构建 `MidiDocument`。
    /// 见 `loader::parsed_midi::build_document_from_midi_bytes`。
    pub fn to_parsed_midi(self) -> ParsedMidi {
        ParsedMidi {
            info: self.info,
            document: None,
        }
    }
}

/// 解析后的MIDI数据
#[derive(Debug, Clone)]
pub struct ParsedMidi {
    pub info: MidiInfo,
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
            info: &'a MidiInfo,
        }
        Helper { info: &self.info }.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for ParsedMidi {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Helper {
            info: MidiInfo,
        }
        let h = Helper::deserialize(deserializer)?;
        Ok(ParsedMidi {
            info: h.info,
            document: None,
        })
    }
}

impl ParsedMidi {
    // midi_data 已在架构层面移除——LMPJ 保存时从内存 document 重建，
    // 不依赖原始 .mid 文件。如需原始 MIDI 字节（如导出标准 MIDI），
    // 调用方应自行从 info.path 读取或从 document 重建。
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
