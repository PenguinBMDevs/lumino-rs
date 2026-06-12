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
    /// DMS 文件解析完成
    DmsParsed(Arc<lumino_midi_loader::ParsedDms>),
    /// DMS 文件解析失败
    DmsParseError(String),
    /* */
    /// 导出 MIDI 文件
    ExportMidi,
    /// 导出 DMS 文件
    ExportDms,
    /// 导出工程为单文件归档 (.lmpj)
    ExportProjectArchive,
    /// 导出工程为文件夹 (.lmpj)
    ExportProjectFolder,
    /// 导出音频文件
    AudioExport,
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
    /// 获取事件的人类可读显示名称
    pub fn display_name(&self) -> String {
        match self {
            Self::New => "新建".to_string(),
            Self::Open => "打开".to_string(),
            Self::Save => "保存".to_string(),
            Self::Close => "关闭".to_string(),
            Self::ImportFiles => "导入文件".to_string(),
            Self::MidiLoaded(_) => "MIDI 加载完成".to_string(),
            Self::MidiLoadError(_) => "MIDI 加载失败".to_string(),
            Self::MidiParsed(_) => "MIDI 解析完成".to_string(),
            Self::MidiParseError(_) => "MIDI 解析失败".to_string(),
            Self::ShowProgress(_, _) => "显示进度".to_string(),
            Self::HideProgress => "隐藏进度".to_string(),
            Self::DmsParsed(_) => "DMS 解析完成".to_string(),
            Self::DmsParseError(_) => "DMS 解析失败".to_string(),
            Self::ExportMidi => "导出 MIDI".to_string(),
            Self::ExportDms => "导出 DMS".to_string(),
            Self::ExportProjectArchive => "导出工程归档".to_string(),
            Self::ExportProjectFolder => "导出工程文件夹".to_string(),
            Self::AudioExport => "音频导出".to_string(),
            Self::ProjectSettings => "工程设置".to_string(),
            Self::Settings => "设置".to_string(),
            Self::Exit => "退出".to_string(),
            Self::TrackSelected(_) => "音轨选择".to_string(),
            #[cfg(debug_assertions)]
            Self::TestMidiLoaded { .. } => "测试 MIDI 加载".to_string(),
        }
    }

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
    pub fn dms_parsed(parsed: Arc<lumino_midi_loader::ParsedDms>) -> Self {
        Self::DmsParsed(parsed)
    }
    pub fn dms_parse_error(err: String) -> Self {
        Self::DmsParseError(err)
    }
    pub const fn export_midi() -> Self {
        Self::ExportMidi
    }
    pub const fn export_dms() -> Self {
        Self::ExportDms
    }
    pub const fn export_project_archive() -> Self {
        Self::ExportProjectArchive
    }
    pub const fn export_project_folder() -> Self {
        Self::ExportProjectFolder
    }
    pub const fn audio_export() -> Self {
        Self::AudioExport
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
