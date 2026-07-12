//! 对话框相关事件

use std::sync::Arc;

use lumino_midi_loader::MidiDocument;

#[derive(Debug, Clone)]
pub enum Event {
    /// 打开自定义精度对话框窗口
    OpenCustomPrecisionDialog,
    /// 打开加载确认对话框
    OpenLoadConfirmDialog {
        path: String,
        size_mb: f64,
    },
    /// 关闭自定义精度对话框窗口
    CloseCustomPrecisionDialog,
    /// 应用自定义精度设置 (numerator, denominator)
    ApplyCustomPrecision(u32, u32),
    /// 打开协作对话框窗口
    OpenCollaborationDialog,
    /// 关闭协作对话框窗口
    CloseCollaborationDialog,
    /// 打开音符变速对话框
    OpenSpeedChangeDialog,
    /// 关闭音符变速对话框
    CloseSpeedChangeDialog,
    /// 确认音符变速
    ConfirmSpeedChange(f32),
    /// 打开工程设置对话框
    OpenProjectSettingsDialog,
    /// 关闭工程设置对话框
    CloseProjectSettingsDialog,
    /// 应用工程设置
    ApplyProjectSettings {
        title: String,
        tempo: f64,
        copyright: String,
    },
    /// 开始音频导出
    ///
    /// 如果 `document` 为 `Some`，则使用内存中的 MidiDocument 进行渲染（零拷贝）；
    /// 否则从 `midi_path` 指定的文件读取。
    StartAudioExport {
        midi_path: String,
        soundfont_path: String,
        output_path: String,
        sample_rate: u32,
        channels: String,
        layer_limit: u32,
        channel_threading: String,
        key_threading: String,
        interpolation: String,
        apply_limiter: bool,
        disable_fade_out: bool,
        linear_envelope: bool,
        /// 内存中的 MidiDocument（如果存在）
        document: Option<Arc<MidiDocument>>,
    },
    /// 开始视频导出
    ///
    /// 视频导出暂用编辑器模式的 MidiDocument 作为数据源（不做流式模式）。
    /// Runner 线程逐帧构建 RenderParams，发送给渲染线程离屏渲染 + GPU 读回，
    /// 再将 BGRA 帧写入 FFmpeg。
    OpenVideoExportDialog,
    CloseVideoExportDialog,
    StartVideoExport {
        output_path: String,
        width: u32,
        height: u32,
        fps: u32,
        container: String,
        codec: String,
        backend: String,
        quality: String,
        /// MIDI 分辨率（PPQ）
        ppq: u16,
        /// 可见键位数（128 或 256，用于 Y 向缩放）
        key_count: u16,
        /// 视频导出渲染模式（"note_rectangle"/"hires_texture"）
        render_mode: String,
        /// 内存中的 MidiDocument（如果存在）
        document: Option<Arc<MidiDocument>>,
    },
}

impl Event {
    pub fn display_name(&self) -> String {
        match self {
            Self::OpenCustomPrecisionDialog => "自定义精度".to_string(),
            Self::OpenLoadConfirmDialog { .. } => "加载确认".to_string(),
            Self::CloseCustomPrecisionDialog => "关闭自定义精度".to_string(),
            Self::ApplyCustomPrecision(_, _) => "应用精度设置".to_string(),
            Self::OpenCollaborationDialog => "协作".to_string(),
            Self::CloseCollaborationDialog => "关闭协作".to_string(),
            Self::OpenSpeedChangeDialog => "音符变速".to_string(),
            Self::CloseSpeedChangeDialog => "关闭音符变速".to_string(),
            Self::ConfirmSpeedChange(_) => "确认变速".to_string(),
            Self::OpenProjectSettingsDialog => "工程设置".to_string(),
            Self::CloseProjectSettingsDialog => "关闭工程设置".to_string(),
            Self::ApplyProjectSettings { .. } => "应用工程设置".to_string(),
            Self::StartAudioExport { document, .. } => {
                if document.is_some() {
                    "音频导出（内存模式）".to_string()
                } else {
                    "音频导出（文件模式）".to_string()
                }
            }
            Self::OpenVideoExportDialog => "打开视频导出对话框".to_string(),
            Self::CloseVideoExportDialog => "关闭视频导出对话框".to_string(),
            Self::StartVideoExport { .. } => "视频导出".to_string(),
        }
    }
}
