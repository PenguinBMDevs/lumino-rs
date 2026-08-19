use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone)]
/// 文件事件
pub enum Event {
    /// 新建工程
    New,
    /// 打开工程
    Open,
    /// 保存工程
    Save,
    /// 关闭当前工程
    Close,
    /// 保存完成（本地写入成功，`path` 为实际保存路径）
    SaveCompleted(PathBuf),
    /// 保存失败（`String` 为错误原因）
    SaveFailed(String),
    /// 保存完成提示超时（3 秒后清除底边栏"文件已经保存"提示）
    SaveHintTimeout,
    /* */
    /// 导入文件
    ImportFiles,
    /// 从云存储导入
    ImportFromCloud,
    /// 保存当前工程到云存储
    SaveToCloud,
    /// MIDI 文件加载完成
    MidiLoaded(lumino_midi_loader::MidiInfo),
    /// MIDI 文件加载失败
    MidiLoadError(String),
    /// MIDI 解析完成
    MidiParsed(Arc<lumino_midi_loader::ParsedMidi>),
    /// MIDI 解析失败
    MidiParseError(String),
    /// 显示进度（消息和进度 0.0-1.0）
    ShowProgress(String, f64),
    /// 隐藏进度条
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
    /// 打开设置对话框
    Settings,
    /* */
    /// 退出应用
    Exit,
    /* */
    /// 音轨切换（从侧边栏选择音轨）
    TrackSelected(usize),
    /* */
    /// 测试模式：MIDI 加载完成
    #[cfg(debug_assertions)]
    TestMidiLoaded {
        /// 解析后的 MIDI 数据
        parsed: Box<lumino_midi_loader::ParsedMidi>,
        /// 测试时长（毫秒）
        test_duration: Option<u64>,
    },
}

impl Event {
    // ── 构造函数（替代 event! 宏） ──

    /// 构造新建工程事件
    pub const fn new_file() -> Self {
        Self::New
    }
    /// 构造打开事件
    pub const fn open() -> Self {
        Self::Open
    }
    /// 构造保存事件
    pub const fn save() -> Self {
        Self::Save
    }
    /// 构造保存完成事件
    pub fn save_completed(path: PathBuf) -> Self {
        Self::SaveCompleted(path)
    }
    /// 构造保存失败事件
    pub fn save_failed(err: String) -> Self {
        Self::SaveFailed(err)
    }
    /// 构造保存提示超时事件
    pub const fn save_hint_timeout() -> Self {
        Self::SaveHintTimeout
    }
    /// 构造关闭事件
    pub const fn close() -> Self {
        Self::Close
    }
    /// 构造导入文件事件
    pub const fn import_files() -> Self {
        Self::ImportFiles
    }
    /// 构造从云存储导入事件
    pub const fn import_from_cloud() -> Self {
        Self::ImportFromCloud
    }
    /// 构造保存到云存储事件
    pub const fn save_to_cloud() -> Self {
        Self::SaveToCloud
    }
    /// 构造 MIDI 加载完成事件
    pub fn midi_loaded(info: lumino_midi_loader::MidiInfo) -> Self {
        Self::MidiLoaded(info)
    }
    /// 构造 MIDI 加载失败事件
    pub fn midi_load_error(err: String) -> Self {
        Self::MidiLoadError(err)
    }
    /// 构造 MIDI 解析完成事件
    pub fn midi_parsed(parsed: Arc<lumino_midi_loader::ParsedMidi>) -> Self {
        Self::MidiParsed(parsed)
    }
    /// 构造 MIDI 解析失败事件
    pub fn midi_parse_error(err: String) -> Self {
        Self::MidiParseError(err)
    }
    /// 构造显示进度事件
    pub fn show_progress(msg: String, progress: f64) -> Self {
        Self::ShowProgress(msg, progress)
    }
    /// 构造隐藏进度事件
    pub const fn hide_progress() -> Self {
        Self::HideProgress
    }
    /// 构造导出 MIDI 事件
    pub const fn export_midi() -> Self {
        Self::ExportMidi
    }
    /// 构造导出工程归档事件
    pub const fn export_project_archive() -> Self {
        Self::ExportProjectArchive
    }
    /// 构造导出工程文件夹事件
    pub const fn export_project_folder() -> Self {
        Self::ExportProjectFolder
    }
    /// 构造导出素材事件
    pub const fn export_material() -> Self {
        Self::ExportMaterial
    }
    /// 构造工程设置事件
    pub const fn project_settings() -> Self {
        Self::ProjectSettings
    }
    /// 构建设置事件
    pub const fn settings() -> Self {
        Self::Settings
    }
    /// 构造退出事件
    pub const fn exit() -> Self {
        Self::Exit
    }
    /// 构造音轨切换事件
    pub const fn track_selected(idx: usize) -> Self {
        Self::TrackSelected(idx)
    }
}
