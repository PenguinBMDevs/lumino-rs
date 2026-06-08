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
    MidiParsed(Arc<crate::ParsedMidi>),
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
        parsed: Box<crate::ParsedMidi>,
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
            Self::MidiLoaded(_) => "MIDI 已加载".to_string(),
            Self::MidiLoadError(_) => "MIDI 加载失败".to_string(),
            Self::MidiParsed(_) => "MIDI 已解析".to_string(),
            Self::MidiParseError(_) => "MIDI 解析失败".to_string(),
            Self::ShowProgress(_, _) => "处理中...".to_string(),
            Self::HideProgress => "隐藏进度".to_string(),
            Self::DmsParsed(_) => "DMS 已解析".to_string(),
            Self::DmsParseError(_) => "DMS 解析失败".to_string(),
            Self::ExportMidi => "导出 MIDI".to_string(),
            Self::ExportDms => "导出 DMS".to_string(),
            Self::ExportProjectArchive => "导出为单文件".to_string(),
            Self::ExportProjectFolder => "导出为文件夹".to_string(),
            Self::AudioExport => "导出音频".to_string(),
            Self::ProjectSettings => "工程设置".to_string(),
            Self::Settings => "设置".to_string(),
            Self::Exit => "退出".to_string(),
            Self::TrackSelected(_) => "音轨切换".to_string(),
            #[cfg(debug_assertions)]
            Self::TestMidiLoaded { .. } => "测试 MIDI 已加载".to_string(),
        }
    }
}
