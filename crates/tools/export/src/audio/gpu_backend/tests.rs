use super::build_synth_config;
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
