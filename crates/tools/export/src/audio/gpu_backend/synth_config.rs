//! GPU 渲染输入构建 — SynthConfig 与 MidiExportData

use super::*;

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

    let channels = match config.channels {
        AudioChannelMode::Mono => ChannelMode::Mono,
        AudioChannelMode::Stereo => ChannelMode::Stereo,
    };

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
    // 全局复音上限与 layer_limit 解耦：CPU/XSynth 基准与实时 LGS 都没有全局上限
    // （`SynthConfig::default().max_voices == 0`），离线导出必须一致。
    // 旧实现 `max_voices: config.layer_limit.unwrap_or(0)` 在默认 32 层时把全局上限
    // 压到 32（物理池 48），高 poly 段被大量抢 voice（issue #31 过度杀音符）。
    let synth_config = SynthConfig {
        sample_rate: config.sample_rate,
        max_voices: 0,
        max_voices_per_key,
        block_size: 512,
        interpolation: map_interpolation(config.interpolation),
        use_effects: true,
        envelope_curves,
        channels,
        render_silence_threshold: 0.0001,
        max_tail_seconds: 120.0,
        show_progress: false,
    };
    tracing::info!(
        "GPU SynthConfig: sample_rate={}, max_voices={} (0=无限制), max_voices_per_key={}, channels={:?}",
        synth_config.sample_rate,
        synth_config.max_voices,
        synth_config.max_voices_per_key,
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
