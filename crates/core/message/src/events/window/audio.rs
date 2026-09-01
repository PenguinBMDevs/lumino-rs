//! 音频导出事件共享类型。

/// 音频通道数。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AudioChannels {
    /// 单声道
    Mono,
    /// 立体声（默认）
    #[default]
    Stereo,
}

impl AudioChannels {
    /// 返回该模式对应的声道数量。
    pub fn channel_count(self) -> u16 {
        match self {
            Self::Mono => 1,
            Self::Stereo => 2,
        }
    }
}

impl std::fmt::Display for AudioChannels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mono => write!(f, "单声道"),
            Self::Stereo => write!(f, "立体声"),
        }
    }
}

/// 多线程选项。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThreadingOption {
    /// 不启用多线程
    None,
    /// 自动（默认）
    #[default]
    Auto,
    /// 手动指定线程数
    Manual(u32),
}

impl std::fmt::Display for ThreadingOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "关闭"),
            Self::Auto => write!(f, "自动"),
            Self::Manual(n) => write!(f, "{n} 线程"),
        }
    }
}

/// 插值算法。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Interpolation {
    /// 无插值
    None,
    /// 线性插值（默认）
    #[default]
    Linear,
}

impl std::fmt::Display for Interpolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "无插值"),
            Self::Linear => write!(f, "线性插值"),
        }
    }
}

/// 音频输出格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AudioFormat {
    /// WAV（默认）
    #[default]
    WAV,
    /// FLAC
    FLAC,
    /// MP3
    MP3,
    /// Ogg Vorbis
    Ogg,
    /// WavPack
    WavPack,
}

impl std::fmt::Display for AudioFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WAV => write!(f, "WAV"),
            Self::FLAC => write!(f, "FLAC"),
            Self::MP3 => write!(f, "MP3"),
            Self::Ogg => write!(f, "Ogg Vorbis"),
            Self::WavPack => write!(f, "WavPack"),
        }
    }
}

impl AudioFormat {
    /// 判断该格式是否需要依赖 ffmpeg 编码
    pub fn needs_ffmpeg(self) -> bool {
        matches!(self, Self::MP3 | Self::Ogg | Self::WavPack)
    }

    /// 返回该格式对应的文件扩展名
    pub fn extension(self) -> &'static str {
        match self {
            Self::WAV => "wav",
            Self::FLAC => "flac",
            Self::MP3 => "mp3",
            Self::Ogg => "ogg",
            Self::WavPack => "wv",
        }
    }
}

/// 音频渲染后端
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AudioBackend {
    /// CPU 后端（xsynth，默认）
    #[default]
    Cpu,
    /// GPU 后端（LGS，需 Vulkan/Metal）
    Gpu,
}

impl std::fmt::Display for AudioBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cpu => write!(f, "CPU (兼容)"),
            Self::Gpu => write!(f, "GPU (加速)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_channels_channel_count() {
        assert_eq!(AudioChannels::Mono.channel_count(), 1);
        assert_eq!(AudioChannels::Stereo.channel_count(), 2);
    }
}
