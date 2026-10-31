//! GPU 渲染输入构建 — SynthConfig 与 MidiExportData

use super::*;

/// REF-LGS-009：离线导出的**默认** GPU 渲染块长（帧）。
///
/// 实际生效值来自 [`AudioRenderConfig::gpu_block_size`]（设置页可改，默认即本
/// 常量），经 [`normalize_gpu_block_size`] 归一化后写入 `SynthConfig`。
///
/// 离线导出对延迟不敏感，块长是最便宜的杠杆（每块固定成本被摊薄）：
/// `sustained65536` 实测 512→2048 快 **2.6×**、512→4096 快 **3.3×**
/// （见 `docs/2026-10-31-REF-LGS-010-011-*` 的 A/B 表）。
///
/// 取 2048 而不是 4096：与 `scripts/lgs_bench_cases.json` 的主用档一致，
/// 且 REF-LGS-008 的分层口径与 limiter 绝对帧窗口（A2）在该档已覆盖验证；
/// 4096 的额外收益只有 ~20%，但每块内存/尾延迟翻倍。
///
/// **实时路径默认块长不变**（`LgsOptions::block_size`，见
/// `crates/audio/midi-io/src/api/lgs/init.rs`）；这里只影响离线导出。
///
/// 合法性由 `SynthConfig::validate` 兜底（2 的幂且 ≥16），非法值在
/// `GpuSynth::new` 处直接报错，不会静默降级。
pub(super) const GPU_OFFLINE_BLOCK_SIZE: usize = 2048;

// 编译期不变量：离线块长必须 ≥2048（否则本卡的目标失效）、是 2 的幂且 ≥16
// （`SynthConfig::validate` 的硬要求）。改常量时在这里直接编译失败。
const _: () = assert!(GPU_OFFLINE_BLOCK_SIZE >= 2048);
const _: () = assert!(GPU_OFFLINE_BLOCK_SIZE.is_power_of_two());
const _: () = assert!(GPU_OFFLINE_BLOCK_SIZE >= 16);

/// 离线块长的合法范围（REF-LGS-009 UI 候选集：512/1024/2048/4096/8192）。
const GPU_OFFLINE_BLOCK_MIN: usize = 512;
const GPU_OFFLINE_BLOCK_MAX: usize = 8192;

/// REF-LGS-009：把导出配置里的块长归一化到 [512, 8192] 的 2 的幂。
///
/// UI 只提供合法候选值；这里兜底旧配置/程序化调用——**不静默**：非法值会
/// warn 一条日志并归一化，避免一次导出因为过期配置直接 `validate` 失败。
fn normalize_gpu_block_size(value: usize) -> usize {
    let normalized = value
        .clamp(GPU_OFFLINE_BLOCK_MIN, GPU_OFFLINE_BLOCK_MAX)
        .next_power_of_two();
    if normalized != value {
        tracing::warn!(
            "GPU 离线块长 {value} 非法，已归一化为 {normalized}（{}..={} 的 2 的幂）",
            GPU_OFFLINE_BLOCK_MIN,
            GPU_OFFLINE_BLOCK_MAX
        );
    }
    normalized
}

/// 将 AudioInterpolation 映射到 GPU InterpolationMode
fn map_interpolation(
    interp: super::config::AudioInterpolation,
) -> lumino_gpu_synth::InterpolationMode {
    use lumino_gpu_synth::InterpolationMode;
    match interp {
        super::config::AudioInterpolation::Nearest => InterpolationMode::Linear,
        super::config::AudioInterpolation::Linear => InterpolationMode::Linear,
    }
}

/// 从 AudioRenderConfig 构建 GPU SynthConfig
pub(super) fn build_synth_config(config: &AudioRenderConfig) -> lumino_gpu_synth::SynthConfig {
    use lumino_gpu_synth::synth::dsp::CurveKind;
    use lumino_gpu_synth::synth::dsp::EnvelopeCurveConfig;
    use lumino_gpu_synth::{ChannelMode, SynthConfig};

    // DEBT-04 #121：GPU 引擎只支持立体声（SynthConfig::validate 拒绝 Mono）。
    // 导出单声道时在写 sink 前真降混（见 gpu_backend::render::write_gpu_result_to_sink），
    // 不再把 ChannelMode::Mono 传给引擎——旧实现静默产出错误音频（仅 warn）。
    let channels = ChannelMode::Stereo;

    let envelope_curves = if config.linear_envelope {
        EnvelopeCurveConfig {
            attack_curve: CurveKind::Exponential,
            decay_curve: CurveKind::Linear,
            release_curve: CurveKind::Linear,
        }
    } else {
        EnvelopeCurveConfig {
            attack_curve: CurveKind::Exponential,
            decay_curve: CurveKind::Exponential,
            release_curve: CurveKind::Exponential,
        }
    };

    // 每键复音与 xsynth 的 SetLayerCount 严格对齐：不做 floor/ceiling 修饰，
    // 用户设 1 就是 1（旧实现 `n.max(4)` 会把 1..3 静默抬到 4，CPU 侧却按原值，
    // 导致 issue #31 的 GPU/CPU 对拍在"参数相同"这个前提下就不成立）。
    // None / Some(0) = 不限制（GPU 侧用 0 表达）。
    let max_voices_per_key = super::config::normalize_layer_limit(config.layer_limit).unwrap_or(0);
    // REND-002 #87：按实际使用的端口块开通全局通道空间（单端口 = 16，
    // 历史行为零变化；超限端口由 B1 折叠到 15 块）。
    let midi_channels =
        lumino_midi_model::multi_port::channels_for_max_port_clamped(config.midi_max_port) as usize;
    // 全局复音上限与 layer_limit 解耦：CPU/XSynth 基准与实时 LGS 都没有全局上限
    // （`SynthConfig::default().max_voices == 0`），离线导出必须一致。
    // 旧实现 `max_voices: config.layer_limit.unwrap_or(0)` 在默认 32 层时把全局上限
    // 压到 32（物理池 48），高 poly 段被大量抢 voice（issue #31 过度杀音符）。
    let synth_config = SynthConfig {
        sample_rate: config.sample_rate,
        max_voices: 0,
        max_voices_per_key,
        midi_channels,
        block_size: normalize_gpu_block_size(config.gpu_block_size),
        interpolation: map_interpolation(config.interpolation),
        use_effects: true,
        envelope_curves,
        channels,
        render_silence_threshold: 0.0001,
        max_tail_seconds: 120.0,
        show_progress: false,
    };
    tracing::info!(
        "GPU SynthConfig: sample_rate={}, block_size={} (离线配置), max_voices={} (0=无限制), max_voices_per_key={}, midi_channels={}, channels={:?}",
        synth_config.sample_rate,
        synth_config.block_size,
        synth_config.max_voices,
        synth_config.max_voices_per_key,
        synth_config.midi_channels,
        synth_config.channels
    );
    synth_config
}

/// 从 MidiDocument 构造 MidiExportData（用于 GPU 临时 MIDI）
pub(super) fn build_export_data(
    doc: &MidiDocument,
    config: &AudioRenderConfig,
) -> crate::midi::MidiExportData {
    use crate::midi::{
        MidiControlChangeEvent, MidiExportData, MidiExportOptions, MidiKeySignatureEvent,
        MidiNoteEvent, MidiPitchBendEvent, MidiProgramChangeEvent, MidiTempoEvent,
        MidiTimeSignatureEvent, MidiTrackData,
    };
    use lumino_midi_loader::bpm_to_tempo;

    let mut pc_by_track: std::collections::HashMap<u16, Vec<MidiProgramChangeEvent>> =
        Default::default();
    let mut cc_by_track: std::collections::HashMap<u16, Vec<MidiControlChangeEvent>> =
        Default::default();
    let mut pb_by_track: std::collections::HashMap<u16, Vec<MidiPitchBendEvent>> =
        Default::default();

    // 仅在非忽略音色时收集 PC/CC/PB；保持文件序（已按 tick 稳定排序）
    // RPN(CC101/100/6/38) 必须在同 tick 的 PB 之前，否则 PB 用错 sensitivity（yinhe 2026-06-27 13:22）。
    // doc.control_events 已稳定排序，迭代序即文件序 + tick 序，push 时 CC 已天然在 PB 前（event_stream 的 priority 同理）。
    for ev in doc.control_events.iter() {
        match ev.kind {
            0 => {
                let (controller, value) = ev.as_control_change();
                cc_by_track
                    .entry(ev.track)
                    .or_default()
                    .push(MidiControlChangeEvent {
                        tick: ev.tick,
                        channel: ev.channel,
                        controller,
                        value,
                    });
            }
            1 => {
                if config.ignore_program_changes {
                    continue;
                }
                let program = ev.as_program_change();
                pc_by_track
                    .entry(ev.track)
                    .or_default()
                    .push(MidiProgramChangeEvent {
                        tick: ev.tick,
                        channel: ev.channel,
                        program,
                    });
            }
            2 => {
                pb_by_track
                    .entry(ev.track)
                    .or_default()
                    .push(MidiPitchBendEvent {
                        tick: ev.tick,
                        channel: ev.channel,
                        value: ev.param,
                    });
            }
            _ => {}
        }
    }

    let tracks: Vec<MidiTrackData> = (0..doc.track_count())
        .map(|i| {
            let track_id = i as u16;
            let mut notes: Vec<MidiNoteEvent> = Vec::new();
            for n in doc.notes[i].iter() {
                // 键位过滤
                if config.filter_key && (n.key < config.key_low || n.key > config.key_high) {
                    continue;
                }
                // 力度过滤
                if config.filter_velocity
                    && (n.velocity < config.velocity_low || n.velocity > config.velocity_high)
                {
                    continue;
                }
                // note_force_end_delay 在 MIDI 层通过延长 duration 体现（毫秒→tick 近似）
                let mut duration = n.length().max(1);
                if config.note_force_end_delay > 0 {
                    // 粗略换算：delay_ms * ppqn * bpm / 60000，取当前文档首 tempo 近似
                    let bpm = doc.tempo_changes.first().map(|(_, b)| *b).unwrap_or(120.0) as f64;
                    let ppqn = doc.division as f64;
                    let extra_ticks =
                        (config.note_force_end_delay as f64 * ppqn * bpm / 60000.0) as u32;
                    duration = duration.saturating_add(extra_ticks);
                }
                notes.push(MidiNoteEvent {
                    tick: n.start_tick,
                    channel: n.channel,
                    key: n.key,
                    velocity: n.velocity,
                    release_velocity: n.release_velocity,
                    duration,
                });
            }
            let (program_changes, control_changes, pitch_bends) = (
                pc_by_track.get(&track_id).cloned().unwrap_or_default(),
                cc_by_track.get(&track_id).cloned().unwrap_or_default(),
                pb_by_track.get(&track_id).cloned().unwrap_or_default(),
            );
            MidiTrackData {
                notes,
                tempos: if i == 0 {
                    doc.tempo_changes
                        .iter()
                        .map(|&(tick, bpm)| MidiTempoEvent {
                            tick,
                            tempo: bpm_to_tempo(bpm as f64),
                        })
                        .collect()
                } else {
                    Vec::new()
                },
                time_signatures: if i == 0 {
                    doc.time_signatures
                        .iter()
                        .map(|&(tick, num, den)| MidiTimeSignatureEvent {
                            tick,
                            numerator: num,
                            denominator: human_denom_to_pow2(den),
                            clocks_per_tick: 24,
                            notated_32nd_notes_per_beat: 8,
                        })
                        .collect()
                } else {
                    Vec::new()
                },
                key_signatures: if i == 0 {
                    doc.key_signatures
                        .iter()
                        .map(|&(tick, sharps, is_minor)| MidiKeySignatureEvent {
                            tick,
                            key: sharps,
                            is_major: !is_minor,
                        })
                        .collect()
                } else {
                    Vec::new()
                },
                program_changes,
                control_changes,
                pitch_bends,
                name: doc.track_name(i).map(|s| s.to_string()),
                // REND-002 #87：FF 21 端口随轨道写回 SMF（writer 已支持）；
                // 端口 0 不写，保持单端口文件无 FF 21 的历史形态。
                midi_port: {
                    let p = doc.track_port(track_id);
                    (p != 0).then_some(p)
                },
                ..Default::default()
            }
        })
        .collect();

    MidiExportData {
        options: MidiExportOptions {
            format: 1,
            ppqn: doc.division,
        },
        tracks,
    }
}

fn human_denom_to_pow2(d: u8) -> u8 {
    match d {
        1 => 0,
        2 => 1,
        4 => 2,
        8 => 3,
        16 => 4,
        32 => 5,
        64 => 6,
        _ => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// REF-LGS-009：构建出的 SynthConfig 必须直接用离线块长，并通过引擎校验。
    #[test]
    fn build_synth_config_uses_offline_block_size() {
        let config = AudioRenderConfig::default();
        let synth = build_synth_config(&config);
        assert_eq!(synth.block_size, GPU_OFFLINE_BLOCK_SIZE);
        assert!(
            synth.validate().is_ok(),
            "默认导出配置必须能通过 SynthConfig::validate"
        );
    }

    /// REF-LGS-009：导出配置的块长生效（设置页可改）；非法值归一化到
    /// [512, 8192] 的 2 的幂后仍能通过 `validate`（不静默失败）。
    #[test]
    fn configured_gpu_block_size_is_used_and_normalized() {
        for (value, expected) in [
            (512usize, 512usize),
            (1024, 1024),
            (2048, 2048),
            (4096, 4096),
            (8192, 8192),
            (0, 512),
            (1000, 1024),
            (16384, 8192),
        ] {
            let config = AudioRenderConfig {
                gpu_block_size: value,
                ..AudioRenderConfig::default()
            };
            let synth = build_synth_config(&config);
            assert_eq!(synth.block_size, expected, "gpu_block_size={value}");
            assert!(
                synth.validate().is_ok(),
                "归一化后必须合法（gpu_block_size={value}）"
            );
        }
    }

    /// CPU 后端的块式渲染参数（`block_frames`）与 GPU 块长解耦：
    /// 改 CPU 侧不会把 GPU 拖回小块长（反之亦然）。
    #[test]
    fn cpu_block_frames_do_not_affect_gpu_block_size() {
        for block_frames in [0u32, 1, 256, 1024] {
            let config = AudioRenderConfig {
                block_frames,
                ..AudioRenderConfig::default()
            };
            let synth = build_synth_config(&config);
            assert_eq!(
                synth.block_size, GPU_OFFLINE_BLOCK_SIZE,
                "block_frames={block_frames} 不应影响 GPU 块长"
            );
        }
    }

    /// 非法块长由引擎边界拒绝（不静默降级）——UI/配置传入非 2 的幂时能定位。
    #[test]
    fn invalid_block_sizes_are_rejected_by_the_engine() {
        let base = build_synth_config(&AudioRenderConfig::default());
        for bad in [0usize, 8, 100, 1000] {
            let mut cfg = base.clone();
            cfg.block_size = bad;
            assert!(
                cfg.validate().is_err(),
                "block_size={bad} 应被 SynthConfig::validate 拒绝"
            );
        }
    }
}
