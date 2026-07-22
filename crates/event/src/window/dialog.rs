//! 对话框相关事件

use std::sync::Arc;

use lumino_midi_loader::MidiDocument;

pub use super::audio::{AudioChannels, AudioFormat, Interpolation, ThreadingOption};
pub use super::video::{
    Container, EncoderBackend, QualityPreset, RenderMode, VideoCodec, VideoExportConfig,
};

/// 音频导出配置。
#[derive(Debug, Clone)]
pub struct AudioExportConfig {
    pub midi_path: String,
    pub soundfont_path: String,
    pub output_path: String,
    pub sample_rate: u32,
    pub channels: AudioChannels,
    pub layer_limit: u32,
    pub channel_threading: ThreadingOption,
    pub key_threading: ThreadingOption,
    pub interpolation: Interpolation,
    pub apply_limiter: bool,
    pub disable_fade_out: bool,
    pub linear_envelope: bool,
    pub audio_format: AudioFormat,
    pub audio_bitrate: u32,
    pub ignore_program_changes: bool,
    pub filter_velocity: bool,
    pub velocity_low: u8,
    pub velocity_high: u8,
    pub filter_key: bool,
    pub key_low: u8,
    pub key_high: u8,
    pub note_force_end_delay: u32,
}

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
    /// 打开批量编辑对话框
    OpenBatchEditDialog,
    /// 关闭批量编辑对话框
    CloseBatchEditDialog,
    /// 确认批量编辑
    ConfirmBatchEdit {
        velocity: String,
        gate: String,
        key: String,
        tick: String,
    },
    /// 打开工程设置对话框
    OpenProjectSettingsDialog,
    /// 关闭工程设置对话框
    CloseProjectSettingsDialog,
    /// 打开内存监控对话框
    OpenMemoryMonitorDialog,
    /// 关闭内存监控对话框
    CloseMemoryMonitorDialog,
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
        config: AudioExportConfig,
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
        /// 视频导出配置（强类型，事件层传输结构）
        config: VideoExportConfig,
        /// 内存中的 MidiDocument（如果存在）
        document: Option<Arc<MidiDocument>>,
    },
}
