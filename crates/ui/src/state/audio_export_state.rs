//! 音频导出面板状态

use lumino_message::{AudioChannels, AudioFormat, Interpolation, ThreadingOption};

/// 音频导出面板状态（主界面侧边栏面板，非独立对话框）
///
/// 纯 UI 状态，仅保存控件值，不含导出处理逻辑。
#[derive(Debug, Clone)]
pub struct AudioExportDialogState {
    /// 工程名称
    pub project_name: String,
    /// MIDI 文件路径
    pub midi_path: String,
    /// SF2 音色库路径
    pub soundfont_path: String,
    /// 采样率
    pub sample_rate: u32,
    /// 通道数
    pub channels: AudioChannels,
    /// 每通道层数限制
    pub layers: u32,
    /// GPU 导出时最大同时 voice 数（0 = 使用默认值 2048）
    pub max_voices: u32,
    /// 通道多线程
    pub channel_threading: ThreadingOption,
    /// 按键多线程
    pub key_threading: ThreadingOption,
    /// 应用限制器
    pub apply_limiter: bool,
    /// 禁用淡出
    pub disable_fade_out: bool,
    /// 线性包络
    pub linear_envelope: bool,
    /// 插值算法
    pub interpolation: Interpolation,
    /// 输出格式
    pub format: AudioFormat,
    /// 编码比特率（kbps，仅 MP3/Vorbis 有效）
    pub audio_bitrate: u32,
    /// 忽略音色变化事件
    pub ignore_program_changes: bool,
    /// 启用音符力度过滤
    pub filter_velocity: bool,
    /// 最低力度
    pub velocity_low: u8,
    /// 最高力度
    pub velocity_high: u8,
    /// 启用键位过滤
    pub filter_key: bool,
    /// 最低键位
    pub key_low: u8,
    /// 最高键位
    pub key_high: u8,
    /// 音符强制结束延迟（毫秒）
    pub note_force_end_delay: u32,
    /// 输出路径
    pub output_path: String,
    /// 是否正在渲染（显示内嵌进度条）
    pub is_rendering: bool,
    /// 渲染进度消息
    pub render_message: String,
    /// 渲染进度 (0.0 - 1.0)
    pub render_progress: f64,
    /// 渲染是否完成
    pub render_completed: bool,
    /// 渲染错误信息
    pub render_error: Option<String>,
}

impl Default for AudioExportDialogState {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioExportDialogState {
    pub fn new() -> Self {
        Self {
            project_name: String::new(),
            midi_path: String::new(),
            soundfont_path: String::new(),
            sample_rate: 48000,
            channels: AudioChannels::default(),
            layers: 32,
            max_voices: 2048,
            channel_threading: ThreadingOption::default(),
            key_threading: ThreadingOption::default(),
            apply_limiter: true,
            disable_fade_out: false,
            linear_envelope: false,
            interpolation: Interpolation::default(),
            format: AudioFormat::default(),
            audio_bitrate: 320,
            ignore_program_changes: false,
            filter_velocity: false,
            velocity_low: 0,
            velocity_high: 127,
            filter_key: false,
            key_low: 0,
            key_high: 127,
            note_force_end_delay: 0,
            output_path: String::new(),
            is_rendering: false,
            render_message: String::new(),
            render_progress: 0.0,
            render_completed: false,
            render_error: None,
        }
    }
}
