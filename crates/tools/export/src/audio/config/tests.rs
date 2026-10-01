//! `AudioRenderConfig` 单元测试（自 `config.rs` 拆出，保持单文件 < 400 行）

use super::*;

#[test]
fn test_layer_limit_from_ui_zero_means_unlimited() {
    assert_eq!(layer_limit_from_ui(0), None);
    assert_eq!(layer_limit_from_ui(1), Some(1));
    assert_eq!(layer_limit_from_ui(32), Some(32));
}

#[test]
fn test_normalize_layer_limit_folds_some_zero_into_none() {
    // Some(0) 是历史遗留的"不限"写法：CPU 与 GPU 必须看到同一个语义，
    // 否则 CPU 侧 SetLayerCount(Some(0)) 会把该键压成"只保留最新一组"。
    assert_eq!(normalize_layer_limit(None), None);
    assert_eq!(normalize_layer_limit(Some(0)), None);
    assert_eq!(normalize_layer_limit(Some(1)), Some(1));
    assert_eq!(normalize_layer_limit(Some(256)), Some(256));
}

#[test]
fn test_audio_channel_mode_channel_count() {
    assert_eq!(AudioChannelMode::Mono.channel_count(), 1);
    assert_eq!(AudioChannelMode::Stereo.channel_count(), 2);
}

#[test]
fn test_audio_channel_mode_into_channel_count() {
    let mono = AudioChannelMode::Mono;
    let stereo = AudioChannelMode::Stereo;
    assert_eq!(ChannelCount::from(mono).count(), 1);
    assert_eq!(ChannelCount::from(stereo).count(), 2);
}

/// REND-002 零行为变化基线：单端口（midi_max_port=0）必须保持 `SynthFormat::Midi`。
#[test]
fn test_synth_format_single_port_stays_midi() {
    let config = AudioRenderConfig::default();
    assert_eq!(
        config.synth_format(),
        SynthFormat::Midi,
        "单端口必须保持 Midi/16 通道（零行为变化基线）"
    );
}

/// REND-002：多端口按 `(min(max_port,15)+1)*16` 开通 Custom 通道，超上限折叠。
#[test]
fn test_synth_format_multi_port_uses_custom_channels() {
    let config = AudioRenderConfig {
        midi_max_port: 6,
        ..Default::default()
    };
    assert_eq!(
        config.synth_format(),
        SynthFormat::Custom { channels: 112 },
        "7 端口素材应为 112 通道"
    );

    let config = AudioRenderConfig {
        midi_max_port: 127,
        ..Default::default()
    };
    assert_eq!(
        config.synth_format(),
        SynthFormat::Custom { channels: 256 },
        "超上限端口应折叠到 16 端口 / 256 通道"
    );
}

/// REND-002：单端口不显式下发打击乐（由 `SynthFormat::Midi` 自动开启）。
#[test]
fn test_percussion_channels_single_port_empty() {
    let config = AudioRenderConfig::default();
    assert!(
        config.percussion_channels().is_empty(),
        "单端口应返回空（引擎自动开 ch9）"
    );
}

/// REND-002：多端口每端口 ch9（全局 `p*16+9`），超上限折叠到端口 15。
#[test]
fn test_percussion_channels_per_port_9() {
    let config = AudioRenderConfig {
        midi_max_port: 6,
        ..Default::default()
    };
    assert_eq!(
        config.percussion_channels(),
        vec![9, 25, 41, 57, 73, 89, 105],
        "7 端口应逐端口初始化 ch9"
    );

    let config = AudioRenderConfig {
        midi_max_port: 127,
        ..Default::default()
    };
    let channels = config.percussion_channels();
    assert_eq!(channels.len(), 16, "超上限折叠为 16 个端口");
    assert_eq!(channels.last().copied(), Some(249), "末端口 ch9 = 15*16+9");
}
