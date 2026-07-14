//! 音频/导出相关类型

// ─── 音频导出相关类型 ───

/// 音频通道数（UI用）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AudioChannels {
    Mono,
    #[default]
    Stereo,
}

impl std::fmt::Display for AudioChannels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioChannels::Mono => write!(f, "单声道"),
            AudioChannels::Stereo => write!(f, "立体声"),
        }
    }
}

/// 多线程选项（UI用）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThreadingOption {
    None,
    #[default]
    Auto,
    Manual(u32),
}

impl std::fmt::Display for ThreadingOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ThreadingOption::None => write!(f, "关闭"),
            ThreadingOption::Auto => write!(f, "自动"),
            ThreadingOption::Manual(n) => write!(f, "{} 线程", n),
        }
    }
}

/// 插值算法（UI用）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Interpolation {
    None,
    #[default]
    Linear,
}

impl std::fmt::Display for Interpolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Interpolation::None => write!(f, "无插值"),
            Interpolation::Linear => write!(f, "线性插值"),
        }
    }
}

/// 音频格式（UI用）— 同步自 lumino_export::audio::codec::AudioCodec
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AudioFormat {
    #[default]
    WAV,
    FLAC,
    MP3,
    Ogg,
    WavPack,
}

impl std::fmt::Display for AudioFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioFormat::WAV => write!(f, "WAV"),
            AudioFormat::FLAC => write!(f, "FLAC"),
            AudioFormat::MP3 => write!(f, "MP3"),
            AudioFormat::Ogg => write!(f, "Ogg Vorbis"),
            AudioFormat::WavPack => write!(f, "WavPack"),
        }
    }
}

impl AudioFormat {
    /// 是否需要 FFmpeg 才能编码
    pub fn needs_ffmpeg(self) -> bool {
        matches!(
            self,
            AudioFormat::MP3 | AudioFormat::Ogg | AudioFormat::WavPack
        )
    }

    /// 获取文件扩展名
    pub fn extension(self) -> &'static str {
        match self {
            AudioFormat::WAV => "wav",
            AudioFormat::FLAC => "flac",
            AudioFormat::MP3 => "mp3",
            AudioFormat::Ogg => "ogg",
            AudioFormat::WavPack => "wv",
        }
    }
}
