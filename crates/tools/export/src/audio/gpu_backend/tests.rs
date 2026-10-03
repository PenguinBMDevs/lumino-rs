use super::{build_export_data, build_synth_config};
use crate::audio::config::AudioRenderConfig;

/// 全局复音上限必须与 layer_limit 解耦：默认 32 层时若把 layer_limit 当全局
/// 上限，高 poly 段会被大量抢 voice（issue #31 过度杀音符）。
#[test]
fn gpu_synth_config_decouples_global_cap_from_layer_limit() {
    for layers in [None, Some(0usize), Some(1), Some(32), Some(256)] {
        let config = AudioRenderConfig {
            layer_limit: layers,
            ..AudioRenderConfig::default()
        };
        let synth = build_synth_config(&config);
        assert_eq!(
            synth.max_voices, 0,
            "全局上限必须与 layer_limit 解耦（layers={layers:?}）"
        );
        let expected = match layers {
            None | Some(0) => 0,
            Some(n) => n,
        };
        assert_eq!(
            synth.max_voices_per_key, expected,
            "每键复音应原样跟随 layer_limit，不得静默抬高（layers={layers:?}）"
        );
    }
}

/// 小值必须原样透传：旧实现 `n.max(4)` 把 UI 的 1..3 静默变成 4，而 CPU
/// （xsynth）按原值走 —— 同一个 UI 值两套行为，对拍验收直接失效。
#[test]
fn gpu_per_key_limit_does_not_silently_raise_small_values() {
    for small in [1usize, 2, 3] {
        let config = AudioRenderConfig {
            layer_limit: Some(small),
            ..AudioRenderConfig::default()
        };
        let synth = build_synth_config(&config);
        assert_eq!(
            synth.max_voices_per_key, small,
            "用户显式设 {small} 时 GPU 不得抬到 4"
        );
    }
}

/// REND-002 #87：通道空间按实际使用端口块开通（单端口=16 零变化）。
#[test]
fn gpu_synth_config_opens_channels_for_used_ports() {
    for (max_port, expected) in [(0u8, 16usize), (1, 32), (6, 112), (15, 256)] {
        let config = AudioRenderConfig {
            midi_max_port: max_port,
            ..AudioRenderConfig::default()
        };
        let synth = build_synth_config(&config);
        assert_eq!(synth.midi_channels, expected, "max_port={max_port}");
    }
}

/// REND-002 #87：导出 SMF 必须携带 FF 21 端口（端口 0 不写）。
#[test]
fn build_export_data_writes_track_ports() {
    use lumino_midi_loader::MidiDocument;

    // 双轨 SMF：轨 0 无 FF21（端口 0），轨 1 FF21=1。
    let mut raw = Vec::new();
    raw.extend_from_slice(b"MThd");
    raw.extend_from_slice(&6u32.to_be_bytes());
    raw.extend_from_slice(&1u16.to_be_bytes());
    raw.extend_from_slice(&2u16.to_be_bytes());
    raw.extend_from_slice(&480u16.to_be_bytes());
    for port in [None, Some(1u8)] {
        let mut track = Vec::new();
        if let Some(p) = port {
            track.extend_from_slice(&[0x00, 0xFF, 0x21, 0x01, p]);
        }
        track.extend_from_slice(&[0x00, 0x90, 0x3C, 0x64]);
        track.extend_from_slice(&[0x83, 0x60, 0x80, 0x3C, 0x40]);
        track.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]);
        raw.extend_from_slice(b"MTrk");
        raw.extend_from_slice(&(track.len() as u32).to_be_bytes());
        raw.extend_from_slice(&track);
    }
    let (doc, _, _) = MidiDocument::from_notes_bytes(&raw, None).expect("双轨 SMF 应可解析");
    let config = AudioRenderConfig::default();
    let data = build_export_data(&doc, &config);
    assert_eq!(data.tracks[0].midi_port, None, "端口 0 不写 FF 21");
    assert_eq!(data.tracks[1].midi_port, Some(1), "轨 1 应携带 FF 21");
}

/// #35：引擎进度必须落在导出总进度的 0.10→0.20（预载）与 0.20→0.85
/// （渲染）区间内，且总量为 0 时不除零/不越界。
#[test]
fn render_progress_maps_into_export_range() {
    use super::progress::render_progress_message;
    use lumino_gpu_synth::RenderProgress;

    let (p, m) = render_progress_message(
        RenderProgress::Prewarm {
            done: 0,
            total: 100,
        },
        0.0,
        None,
    );
    assert!((p - 0.10).abs() < 1e-9, "预载起点 {p}");
    assert!(m.contains("预载"), "预载文案: {m}");

    let (p, _) = render_progress_message(
        RenderProgress::Prewarm {
            done: 100,
            total: 100,
        },
        0.0,
        None,
    );
    assert!((p - 0.20).abs() < 1e-9, "预载终点 {p}");

    let (p, _) = render_progress_message(RenderProgress::Render { done: 0, total: 0 }, 0.0, None);
    assert!((p - 0.20).abs() < 1e-9, "total=0 不应越界 {p}");

    let (p, m) = render_progress_message(
        RenderProgress::Render {
            done: 500,
            total: 1000,
        },
        0.0,
        Some(3.25),
    );
    assert!((p - 0.525).abs() < 1e-9, "渲染中点 {p}");
    assert!(m.contains("进度: 52.5%"), "文案百分比应与进度条一致: {m}");
    assert!(m.contains("3.25× 实时"), "文案应含倍速: {m}");

    let (p, _) = render_progress_message(
        RenderProgress::Render {
            done: 2000,
            total: 1000,
        },
        0.0,
        None,
    );
    assert!((p - 0.85).abs() < 1e-9, "渲染封顶 {p}");
}
