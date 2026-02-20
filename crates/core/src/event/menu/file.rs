use std::sync::Arc;

#[derive(Debug, Clone)]
/// 文件事件
pub enum Event {
    New,
    Open,
    Save,
    Close,
    /* */
    ImportFiles,
    MidiLoaded(crate::MidiInfo),
    MidiLoadError(String),
    MidiParsed(crate::ParsedMidi),
    MidiParseError(String),
    ShowProgress(String, f64), // 消息和进度 0.0-1.0
    HideProgress,
    /* */
    /// DMS 文件解析完成
    DmsParsed(Arc<crate::ParsedDms>),
    /// DMS 文件解析失败
    DmsParseError(String),
    /* */
    /// 导出 MIDI 文件
    ExportMidi,
    /// 导出 DMS 文件
    ExportDms,
    /* */
    Settings,
    /* */
    Exit,
}
