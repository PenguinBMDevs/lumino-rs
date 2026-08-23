//! MIDI 加载器 crate。
//!
//! 负责 MIDI / LMPJ 文件的解析、压缩包解压、流式/全量加载，
//! 以及工程数据的序列化（`LmpjData`）。`load_parsed_midi` 是主要入口。

pub mod archive;
pub mod constants;
#[cfg(test)]
pub mod document_tests;
pub mod event;
pub mod info;
pub mod loader;
pub mod quantize;
pub mod streaming;

// 重新导出 lumino-midi-model 中的类型（调用链保持 lumino_midi_loader::Xxx 不变）
pub use lumino_midi_model::{
    ChunkedList, CompactEvent, EventKind, LoaderError, LoaderResult, MidiDocument, NoteEvent,
    NoteInfo, TICK_SEARCH_BUFFER, TrackManager, TrackNoteView, TrackView, TrackVisibility,
};

pub use event::MidiEvent;
pub use info::MidiInfo;
pub use streaming::StreamingMidiPlayer;

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
    /// MIDI 元信息（路径、音轨数、音符数、时长等）。
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
            // 旧版 LMPJ 格式无 stats 段，历史累计时间按 0 处理
            accumulated_editing_secs: 0.0,
            // 旧版 LMPJ 格式无作者/版权字段，按空处理
            author: String::new(),
            copyright: String::new(),
        }
    }
}

/// 解析后的MIDI数据
#[derive(Debug, Clone)]
pub struct ParsedMidi {
    /// MIDI 元信息（路径、音轨数、音符数、时长等）。
    pub info: MidiInfo,
    /// 紧凑内存解析结果（常规 MIDI 文件使用）
    ///
    /// 使用 `Arc<MidiDocument>` 而非裸 `MidiDocument`，使得 UI 可以通过 Arc::clone
    /// 共享同一份事件数据而无需深拷贝 `document.clone()`（后者会复制 `Vec<CompactEvent>`，
    /// 对 10M 事件的黑乐谱来说就是额外 120MB）。
    pub document: Option<Arc<MidiDocument>>,
    /// 历史累计创作时间（秒）
    ///
    /// 从 `.lmpj` 工程文件 `metadata.stats.working_time_seconds` 加载，
    /// 供 Runner 注入 `SessionTracker.accumulated_editing_secs`，实现跨会话累计；
    /// 常规 MIDI 文件加载时为 0。
    /// 不参与序列化（旧格式兼容：反序列化后为 0）。
    pub accumulated_editing_secs: f64,
    /// 工程作者（仅 `.lmpj` 工程文件携带，常规 MIDI 文件加载时为空）。
    ///
    /// 由 `project_to_parsed_midi` 从工程元数据 `metadata.project.author` 回填，
    /// 供 Runner 加载后恢复工程设置面板的作者字段。不参与序列化。
    pub author: String,
    /// 工程版权信息（仅 `.lmpj` 工程文件携带，常规 MIDI 文件加载时为空）。
    ///
    /// 由 `project_to_parsed_midi` 从工程元数据 `metadata.project.copyright` 回填，
    /// 供 Runner 加载后恢复工程设置面板的版权字段。不参与序列化。
    pub copyright: String,
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
        let helper = Helper::deserialize(deserializer)?;
        Ok(ParsedMidi {
            info: helper.info,
            document: None,
            // 旧数据没有该字段，反序列化后按 0 处理
            accumulated_editing_secs: 0.0,
            // 旧数据没有作者/版权字段，反序列化后按空处理
            author: String::new(),
            copyright: String::new(),
        })
    }
}
