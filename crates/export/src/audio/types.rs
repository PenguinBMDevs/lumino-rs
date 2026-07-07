//! 音频导出功能——类型定义

/// 音频导出格式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AudioFormat {
    #[default]
    WAV,
    FLAC,
}

impl std::fmt::Display for AudioFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioFormat::WAV => write!(f, "WAV"),
            AudioFormat::FLAC => write!(f, "FLAC"),
        }
    }
}

/// 音频通道数
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AudioChannels {
    Mono,
    #[default]
    Stereo,
}

impl AudioChannels {
    pub fn count(&self) -> u16 {
        match self {
            AudioChannels::Mono => 1,
            AudioChannels::Stereo => 2,
        }
    }
}

impl std::fmt::Display for AudioChannels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioChannels::Mono => write!(f, "单声道"),
            AudioChannels::Stereo => write!(f, "立体声"),
        }
    }
}

/// 多线程选项
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThreadingOption {
    None,
    #[default]
    Auto,
    Manual(u32),
}

/// 插值算法
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Interpolation {
    None,
    #[default]
    Linear,
}

/// 音频导出选项
#[derive(Debug, Clone)]
pub struct AudioExportOptions {
    /// 采样率 (Hz)
    pub sample_rate: u32,
    /// 音频通道数
    pub channels: AudioChannels,
    /// 每通道层数限制 (0 = 无限制)
    pub layers: u32,
    /// GPU 导出时最大同时 voice 数（0 = 使用默认值 2048）
    pub max_voices: u32,
    /// 通道多线程选项
    pub channel_threading: ThreadingOption,
    /// 按键多线程选项
    pub key_threading: ThreadingOption,
    /// 应用限制器防削波
    pub apply_limiter: bool,
    /// 禁用淡出（可能爆音）
    pub disable_fade_out: bool,
    /// 线性包络
    pub linear_envelope: bool,
    /// 插值算法
    pub interpolation: Interpolation,
    /// 输出格式
    pub format: AudioFormat,
    /// 是否使用 GPU 加速渲染（旁路开关：关闭时回退到 CPU 渲染）
    pub use_gpu: bool,
}

impl Default for AudioExportOptions {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            channels: AudioChannels::default(),
            // 从 32 降至 8：降低默认 voice 层数，防止黑乐谱场景 OOM
            layers: 8,
            // GPU 导出默认 voice 上限，避免密集 MIDI 下音符被静默丢弃
            max_voices: 2048,
            channel_threading: ThreadingOption::default(),
            key_threading: ThreadingOption::default(),
            apply_limiter: false,
            disable_fade_out: false,
            linear_envelope: false,
            interpolation: Interpolation::default(),
            format: AudioFormat::default(),
            // 默认启用 GPU 加速；出现兼容性问题时可通过 UI 关闭回退 CPU
            use_gpu: true,
        }
    }
}

/// 音频导出进度信息
#[derive(Debug, Clone, Copy, Default)]
pub struct ExportProgress {
    pub progress: f32,
    pub note_on: u64,
    pub note_off: u64,
}

/// 音频导出进度回调类型
pub type ProgressCallback = Box<dyn Fn(ExportProgress) + Send + Sync>;

/// 最大渲染块大小（秒）。限制单次分配避免 OOM。
pub(super) const MAX_RENDER_CHUNK_SECONDS: f64 = 1.0;
