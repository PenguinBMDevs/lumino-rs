use super::types::*;

#[test]
fn test_audio_format_display() {
    assert_eq!(AudioFormat::WAV.to_string(), "WAV");
    assert_eq!(AudioFormat::FLAC.to_string(), "FLAC");
}

#[test]
fn test_audio_channels_count() {
    assert_eq!(AudioChannels::Mono.count(), 1);
    assert_eq!(AudioChannels::Stereo.count(), 2);
}

#[test]
fn test_audio_export_options_default() {
    let options = AudioExportOptions::default();
    assert_eq!(options.sample_rate, 48000);
    assert_eq!(options.channels, AudioChannels::Stereo);
    assert_eq!(options.layers, 8);
    assert!(!options.apply_limiter);
    assert_eq!(options.format, AudioFormat::WAV);
}
