//! 渲染配置组装：UI 枚举映射到导出 crate 的配置类型。

use std::path::PathBuf;
use std::sync::Arc;

use lumino_export::audio::codec::AudioCodec;
use lumino_export::audio::config::{
    AudioBackendKind, AudioChannelMode, AudioInterpolation, AudioRenderConfig, ProgressCallback,
    ThreadMode, layer_limit_from_ui,
};
use lumino_export::audio::control::AudioExportControl;
use lumino_message::events::window::audio::{
    AudioBackend, AudioChannels, AudioFormat, Interpolation, ThreadingOption,
};
use lumino_message::events::window::dialog::AudioExportConfig;
use lumino_midi_model::multi_port::PortSoundfontMap;

/// 声道模式映射。
fn to_channel_mode(channels: AudioChannels) -> AudioChannelMode {
    match channels {
        AudioChannels::Mono => AudioChannelMode::Mono,
        AudioChannels::Stereo => AudioChannelMode::Stereo,
    }
}

/// 插值算法映射。
fn to_interpolation(interpolation: Interpolation) -> AudioInterpolation {
    match interpolation {
        Interpolation::None => AudioInterpolation::Nearest,
        Interpolation::Linear => AudioInterpolation::Linear,
    }
}

/// 多线程选项映射（通道级 / 按键级共用）。
fn to_thread_mode(option: ThreadingOption) -> ThreadMode {
    match option {
        ThreadingOption::None => ThreadMode::None,
        ThreadingOption::Auto => ThreadMode::Auto,
        ThreadingOption::Manual(n) => ThreadMode::Manual(n),
    }
}

/// 输出格式映射。
fn to_audio_codec(format: AudioFormat) -> AudioCodec {
    match format {
        AudioFormat::WAV => AudioCodec::Pcm,
        AudioFormat::FLAC => AudioCodec::Flac,
        AudioFormat::MP3 => AudioCodec::Mp3,
        AudioFormat::Ogg => AudioCodec::Vorbis,
        AudioFormat::WavPack => AudioCodec::WavPack,
    }
}

/// 后端选择映射。
fn to_backend_kind(backend: AudioBackend) -> AudioBackendKind {
    match backend {
        AudioBackend::Cpu => AudioBackendKind::Cpu,
        AudioBackend::Gpu => AudioBackendKind::Gpu,
    }
}

/// 将 UI 配置组装为渲染配置。
pub(super) fn build_render_config(
    config: AudioExportConfig,
    gpu_block_size: usize,
    progress_cb: ProgressCallback,
    control: &Arc<AudioExportControl>,
    port_soundfonts: PortSoundfontMap,
) -> AudioRenderConfig {
    let AudioExportConfig {
        midi_path,
        soundfont_path,
        output_path,
        sample_rate,
        channels,
        layer_limit,
        channel_threading,
        key_threading,
        interpolation,
        apply_limiter,
        disable_fade_out,
        linear_envelope,
        audio_format,
        audio_bitrate,
        ignore_program_changes,
        filter_velocity,
        velocity_low,
        velocity_high,
        filter_key,
        key_low,
        key_high,
        note_force_end_delay,
        backend,
    } = config;

    let midi_path_buf = PathBuf::from(&midi_path);
    let output_path_buf = PathBuf::from(&output_path);

    let channel_mode = to_channel_mode(channels);
    let interpolation_val = to_interpolation(interpolation);
    let channel_threading_val = to_thread_mode(channel_threading);
    let key_threading_val = to_thread_mode(key_threading);
    let audio_codec = to_audio_codec(audio_format);
    let backend_kind = to_backend_kind(backend);

    AudioRenderConfig {
        midi_path: midi_path_buf,
        soundfonts: vec![PathBuf::from(&soundfont_path)],
        output_path: output_path_buf,
        sample_rate: sample_rate.max(8000),
        channels: channel_mode,
        layer_limit: layer_limit_from_ui(layer_limit),
        channel_threading: channel_threading_val,
        key_threading: key_threading_val,
        interpolation: interpolation_val,
        apply_limiter,
        disable_fade_out,
        linear_envelope,
        audio_codec,
        audio_bitrate,
        ignore_program_changes,
        filter_velocity,
        velocity_low,
        velocity_high,
        filter_key,
        key_low,
        key_high,
        note_force_end_delay,
        // PREF-002：CPU 块式渲染块大小（默认 256 帧；0/1 = 逐事件精确模式）
        block_frames: 256,
        // REF-LGS-009：GPU 离线导出块长（设置页可改，默认 2048；只影响 GPU 后端）
        gpu_block_size,
        // REND-002：多端口由渲染入口按文档/文件推导（此处不感知文档，保持 0）。
        midi_max_port: 0,
        backend: backend_kind,
        progress_callback: Some(progress_cb),
        control: Some(Arc::clone(control)),
        // REND-005：端口音色库映射来自工程态；GPU 后端会在渲染入口显式拒绝。
        port_soundfonts,
    }
}
