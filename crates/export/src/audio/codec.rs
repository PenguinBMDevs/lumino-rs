//! 音频编码器支持 — 参考 OmniConverter 的 AudioCodecType
//!
//! 提供 WAV/FLAC/MP3/Ogg/WavPack 等多格式支持。
//! MP3/Ogg/FLAC/WavPack 通过 FFmpeg 子进程编码，WAV 通过 hound 直接写入。

use std::path::PathBuf;

/// 音频编码器类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCodec {
    /// PCM WAV（默认，不依赖外部工具）
    Pcm,
    /// FLAC（需要 ffmpeg）
    Flac,
    /// MP3 (LAME)（需要 ffmpeg）
    Mp3,
    /// Vorbis / OGG（需要 ffmpeg）
    Vorbis,
    /// WavPack（需要 ffmpeg）
    WavPack,
}

impl AudioCodec {
    /// 返回文件扩展名（含点）
    pub fn extension(&self) -> &'static str {
        match self {
            AudioCodec::Pcm => ".wav",
            AudioCodec::Flac => ".flac",
            AudioCodec::Mp3 => ".mp3",
            AudioCodec::Vorbis => ".ogg",
            AudioCodec::WavPack => ".wv",
        }
    }

    /// 该编码器是否能处理浮点样本
    pub fn supports_float(&self) -> bool {
        match self {
            AudioCodec::Flac | AudioCodec::Mp3 => false,
            _ => true,
        }
    }

    /// 是否提供比特率设置
    pub fn has_bitrate(&self) -> bool {
        matches!(self, AudioCodec::Mp3 | AudioCodec::Vorbis)
    }

    /// 获取 ffmpeg 编码器名称
    pub fn ffmpeg_codec(&self) -> Option<&'static str> {
        match self {
            AudioCodec::Pcm => None,
            AudioCodec::Flac => Some("flac"),
            AudioCodec::Mp3 => Some("libmp3lame"),
            AudioCodec::Vorbis => Some("libvorbis"),
            AudioCodec::WavPack => Some("wavpack"),
        }
    }

    /// 验证采样率和比特率是否在编码器支持范围内
    pub fn validate(&self, sample_rate: u32, bitrate: u32) -> Result<(), String> {
        match self {
            AudioCodec::Pcm | AudioCodec::WavPack => Ok(()),
            AudioCodec::Flac => {
                if sample_rate > 384000 {
                    return Err("FLAC 不支持超过 384kHz 的采样率".into());
                }
                Ok(())
            }
            AudioCodec::Mp3 => {
                let mut errors = Vec::new();
                if sample_rate > 48000 {
                    errors.push("MP3 不支持超过 48kHz 的采样率");
                }
                if bitrate > 320 {
                    errors.push("MP3 不支持超过 320kbps 的比特率");
                }
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(errors.join("\n"))
                }
            }
            AudioCodec::Vorbis => {
                let mut errors = Vec::new();
                if sample_rate > 48000 {
                    errors.push("Vorbis 不支持超过 48kHz 的采样率");
                }
                if bitrate > 480 {
                    errors.push("Vorbis 不支持超过 480kbps 的比特率");
                }
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(errors.join("\n"))
                }
            }
        }
    }

    /// 是否需要 ffmpeg 编码
    pub fn needs_ffmpeg(&self) -> bool {
        !matches!(self, AudioCodec::Pcm)
    }
}

/// 默认编码器
impl Default for AudioCodec {
    fn default() -> Self {
        AudioCodec::Pcm
    }
}

/// 在文件系统中查找 ffmpeg 可执行文件
pub fn find_ffmpeg() -> Option<PathBuf> {
    // 优先检查程序目录
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let local = dir.join("ffmpeg.exe");
            if local.exists() {
                return Some(local);
            }
            let local = dir.join("ffmpeg");
            if local.exists() {
                return Some(local);
            }
        }
    }

    // 回退到 PATH 搜索（Windows / Linux / macOS）
    if let Ok(path) = std::env::var("PATH") {
        for p in std::env::split_paths(&path) {
            let candidate = p.join("ffmpeg.exe");
            if candidate.exists() {
                return Some(candidate);
            }
            let candidate = p.join("ffmpeg");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    None
}

/// 检查 ffmpeg 是否可用
pub fn is_ffmpeg_available() -> bool {
    find_ffmpeg().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codec_extension() {
        assert_eq!(AudioCodec::Pcm.extension(), ".wav");
        assert_eq!(AudioCodec::Flac.extension(), ".flac");
        assert_eq!(AudioCodec::Mp3.extension(), ".mp3");
        assert_eq!(AudioCodec::Vorbis.extension(), ".ogg");
        assert_eq!(AudioCodec::WavPack.extension(), ".wv");
    }

    #[test]
    fn test_codec_supports_float() {
        assert!(AudioCodec::Pcm.supports_float());
        assert!(!AudioCodec::Flac.supports_float());
        assert!(!AudioCodec::Mp3.supports_float());
        assert!(AudioCodec::Vorbis.supports_float());
        assert!(AudioCodec::WavPack.supports_float());
    }

    #[test]
    fn test_codec_validate() {
        assert!(AudioCodec::Pcm.validate(96000, 0).is_ok());
        assert!(AudioCodec::Mp3.validate(48000, 320).is_ok());
        assert!(AudioCodec::Mp3.validate(96000, 320).is_err());
        assert!(AudioCodec::Mp3.validate(48000, 500).is_err());
    }

    #[test]
    fn test_codec_needs_ffmpeg() {
        assert!(!AudioCodec::Pcm.needs_ffmpeg());
        assert!(AudioCodec::Flac.needs_ffmpeg());
        assert!(AudioCodec::Mp3.needs_ffmpeg());
    }
}
