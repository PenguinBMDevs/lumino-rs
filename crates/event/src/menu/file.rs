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
    MidiLoaded(lumino_midi_loader::MidiInfo),
    MidiLoadError(String),
    MidiParsed(Arc<lumino_midi_loader::ParsedMidi>),
    MidiParseError(String),
    ShowProgress(String, f64), // 消息和进度 0.0-1.0
    HideProgress,
    /* */
    /// 导出 MIDI 文件
    ExportMidi,
    /// 导出工程为单文件归档 (.lmpj)
    ExportProjectArchive,
    /// 导出工程为文件夹 (.lmpj)
    ExportProjectFolder,
    /// 导出选中音符为素材 (.lmmaterial)
    ExportMaterial,
    /// 工程设置
    ProjectSettings,
    /* */
    Settings,
    /* */
    Exit,
    /* */
    /// 音轨切换（从侧边栏选择音轨）
    TrackSelected(usize),
    /* */
    /// 测试模式：MIDI 加载完成
    #[cfg(debug_assertions)]
    TestMidiLoaded {
        parsed: Box<lumino_midi_loader::ParsedMidi>,
        test_duration: Option<u64>,
    },
}

impl Event {
    // ── 构造函数（替代 event! 宏） ──

    pub const fn new_file() -> Self {
        Self::New
    }
    pub const fn open() -> Self {
        Self::Open
    }
    pub const fn save() -> Self {
        Self::Save
    }
    pub const fn close() -> Self {
        Self::Close
    }
    pub const fn import_files() -> Self {
        Self::ImportFiles
    }
    pub fn midi_loaded(info: lumino_midi_loader::MidiInfo) -> Self {
        Self::MidiLoaded(info)
    }
    pub fn midi_load_error(err: String) -> Self {
        Self::MidiLoadError(err)
    }
    pub fn midi_parsed(parsed: Arc<lumino_midi_loader::ParsedMidi>) -> Self {
        Self::MidiParsed(parsed)
    }
    pub fn midi_parse_error(err: String) -> Self {
        Self::MidiParseError(err)
    }
    pub fn show_progress(msg: String, progress: f64) -> Self {
        Self::ShowProgress(msg, progress)
    }
    pub const fn hide_progress() -> Self {
        Self::HideProgress
    }
    pub const fn export_midi() -> Self {
        Self::ExportMidi
    }
    pub const fn export_project_archive() -> Self {
        Self::ExportProjectArchive
    }
    pub const fn export_project_folder() -> Self {
        Self::ExportProjectFolder
    }
    pub const fn export_material() -> Self {
        Self::ExportMaterial
    }
    pub const fn project_settings() -> Self {
        Self::ProjectSettings
    }
    pub const fn settings() -> Self {
        Self::Settings
    }
    pub const fn exit() -> Self {
        Self::Exit
    }
    pub const fn track_selected(idx: usize) -> Self {
        Self::TrackSelected(idx)
    }
}
