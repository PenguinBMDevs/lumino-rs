//! 对话框相关事件

use std::sync::Arc;

use lumino_midi_loader::MidiDocument;

pub use super::audio::{AudioBackend, AudioChannels, AudioFormat, Interpolation, ThreadingOption};
pub use super::video::{
    Container, EncoderBackend, QualityPreset, RenderMode, VideoCodec, VideoExportConfig,
};

/// 音频导出配置。
#[derive(Debug, Clone)]
pub struct AudioExportConfig {
    /// MIDI 源文件路径
    pub midi_path: String,
    /// 音色库文件路径
    pub soundfont_path: String,
    /// 输出文件路径
    pub output_path: String,
    /// 采样率
    pub sample_rate: u32,
    /// 通道数
    pub channels: AudioChannels,
    /// 层数限制
    pub layer_limit: u32,
    /// 通道多线程选项
    pub channel_threading: ThreadingOption,
    /// 按键多线程选项
    pub key_threading: ThreadingOption,
    /// 插值算法
    pub interpolation: Interpolation,
    /// 是否应用限制器
    pub apply_limiter: bool,
    /// 是否禁用淡出
    pub disable_fade_out: bool,
    /// 是否启用线性包络
    pub linear_envelope: bool,
    /// 输出音频格式
    pub audio_format: AudioFormat,
    /// 音频比特率
    pub audio_bitrate: u32,
    /// 是否忽略音色变化事件
    pub ignore_program_changes: bool,
    /// 是否启用音符力度过滤
    pub filter_velocity: bool,
    /// 最低力度阈值
    pub velocity_low: u8,
    /// 最高力度阈值
    pub velocity_high: u8,
    /// 是否启用键位过滤
    pub filter_key: bool,
    /// 最低键位阈值
    pub key_low: u8,
    /// 最高键位阈值
    pub key_high: u8,
    /// 音符强制结束延迟（毫秒）
    pub note_force_end_delay: u32,
    /// 渲染后端（CPU / GPU）
    pub backend: AudioBackend,
}

#[derive(Debug, Clone)]
/// 对话框事件
pub enum Event {
    /// 打开自定义精度对话框窗口
    OpenCustomPrecisionDialog,
    /// 打开画刷「绘制行为」对话框窗口（携带当前画刷配置）
    OpenBrushSettingsDialog(lumino_core::BrushConfig),
    /// 打开加载确认对话框
    OpenLoadConfirmDialog {
        /// 文件路径
        path: String,
        /// 文件大小（MB）
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
        /// 力度编辑值
        velocity: String,
        /// 门限编辑值
        gate: String,
        /// 键位编辑值
        key: String,
        /// tick 编辑值
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
    /// 打开"找回删除音轨"对话框
    ///
    /// 实际的条目列表由 Runner 在对话框 UI 就绪后扫描缓存目录并填充。
    OpenRecoverTrackDialog,
    /// 关闭"找回删除音轨"对话框
    CloseRecoverTrackDialog,
    /// 应用工程设置
    ApplyProjectSettings {
        /// 工程标题
        title: String,
        /// 速度（BPM）
        tempo: f64,
        /// 版权信息
        copyright: String,
        /// 作者
        author: String,
        /// 拍号变化列表 (tick, 分子, 分母)
        time_signatures: Vec<(u32, u8, u8)>,
    },
    /// 开始音频导出
    ///
    /// 如果 `document` 为 `Some`，则使用内存中的 MidiDocument 进行渲染（零拷贝）；
    /// 否则从 `midi_path` 指定的文件读取。
    StartAudioExport {
        /// 音频导出配置（Box 减小 Message 枚举体积，见 `window::Event` 布局注释）
        config: Box<AudioExportConfig>,
        /// 内存中的 MidiDocument（如果存在）
        document: Option<Arc<MidiDocument>>,
    },
    /// 打开视频导出对话框
    OpenVideoExportDialog,
    /// 关闭视频导出对话框
    CloseVideoExportDialog,
    /// 开始视频导出
    ///
    /// 视频导出暂用编辑器模式的 MidiDocument 作为数据源（不做流式模式）。
    /// Runner 线程逐帧构建 RenderParams，发送给渲染线程离屏渲染 + GPU 读回，
    /// 再将 BGRA 帧写入 FFmpeg。
    StartVideoExport {
        /// 视频导出配置（强类型，事件层传输结构；Box 减小 Message 枚举体积）
        config: Box<VideoExportConfig>,
        /// 内存中的 MidiDocument（如果存在）
        document: Option<Arc<MidiDocument>>,
    },
}
