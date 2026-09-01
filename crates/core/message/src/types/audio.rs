//! 音频/导出相关类型。

pub use crate::events::window::audio::{
    AudioBackend, AudioChannels, AudioFormat, Interpolation, ThreadingOption,
};

#[cfg(test)]
mod tests {
    use super::*;

    // ─── AudioChannels ───

    #[test]
    fn test_audio_channels_default() {
        assert_eq!(AudioChannels::default(), AudioChannels::Stereo);
    }

    #[test]
    fn test_audio_channels_display() {
        assert_eq!(AudioChannels::Mono.to_string(), "单声道");
        assert_eq!(AudioChannels::Stereo.to_string(), "立体声");
    }

    #[test]
    fn test_audio_channels_channel_count() {
        assert_eq!(AudioChannels::Mono.channel_count(), 1);
        assert_eq!(AudioChannels::Stereo.channel_count(), 2);
    }

    // ─── AudioFormat ───

    #[test]
    fn test_audio_format_default() {
        assert_eq!(AudioFormat::default(), AudioFormat::WAV);
    }

    #[test]
    fn test_audio_format_display() {
        assert_eq!(AudioFormat::WAV.to_string(), "WAV");
        assert_eq!(AudioFormat::FLAC.to_string(), "FLAC");
        assert_eq!(AudioFormat::MP3.to_string(), "MP3");
        assert_eq!(AudioFormat::Ogg.to_string(), "Ogg Vorbis");
        assert_eq!(AudioFormat::WavPack.to_string(), "WavPack");
    }

    #[test]
    fn test_audio_format_extension() {
        assert_eq!(AudioFormat::WAV.extension(), "wav");
        assert_eq!(AudioFormat::FLAC.extension(), "flac");
        assert_eq!(AudioFormat::MP3.extension(), "mp3");
        assert_eq!(AudioFormat::Ogg.extension(), "ogg");
        assert_eq!(AudioFormat::WavPack.extension(), "wv");
    }

    #[test]
    fn test_audio_format_needs_ffmpeg() {
        assert!(!AudioFormat::WAV.needs_ffmpeg());
        assert!(!AudioFormat::FLAC.needs_ffmpeg());
        assert!(AudioFormat::MP3.needs_ffmpeg());
        assert!(AudioFormat::Ogg.needs_ffmpeg());
        assert!(AudioFormat::WavPack.needs_ffmpeg());
    }

    // ─── ThreadingOption ───

    #[test]
    fn test_threading_option_display() {
        assert_eq!(ThreadingOption::None.to_string(), "关闭");
        assert_eq!(ThreadingOption::Auto.to_string(), "自动");
        assert_eq!(ThreadingOption::Manual(4).to_string(), "4 线程");
    }

    // ─── Interpolation ───

    #[test]
    fn test_interpolation_default() {
        assert_eq!(Interpolation::default(), Interpolation::Linear);
    }

    #[test]
    fn test_interpolation_display() {
        assert_eq!(Interpolation::None.to_string(), "无插值");
        assert_eq!(Interpolation::Linear.to_string(), "线性插值");
    }
}
