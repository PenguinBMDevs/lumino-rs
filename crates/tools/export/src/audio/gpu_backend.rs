//! GPU 音频导出后端 — 基于 lumino-gpu-synth
//!
//! 提供与 CPU (xsynth) 对等的离线渲染能力，支持通过 `AudioRenderConfig` 选择后端。
//! 架构：`MidiDocument` → `MidiExportData` → 临时 MIDI 文件 → `GpuSynth::render_midi_file` → `SampleSink`。

use lumino_midi_loader::MidiDocument;

use crate::error::{ExportError, ExportResult};

use super::config::{AudioChannelMode, AudioRenderConfig};
use super::sink_factory::create_output_sink;

/// 检测 GPU 是否可用（尝试创建 wgpu 适配器）
pub fn is_gpu_available() -> bool {
    lumino_gpu_synth::gpu::create_gpu_context().is_ok()
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
fn build_synth_config(config: &AudioRenderConfig) -> lumino_gpu_synth::SynthConfig {
    use lumino_gpu_synth::{ChannelMode, SynthConfig};
    use lumino_gpu_synth::synth::dsp::EnvelopeCurveConfig;
    use lumino_gpu_synth::synth::dsp::CurveKind;

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

    SynthConfig {
        sample_rate: config.sample_rate,
        max_voices: config.layer_limit.unwrap_or(0),
        max_voices_per_key: 4,
        block_size: 512,
        interpolation: map_interpolation(config.interpolation),
        use_effects: true,
        envelope_curves,
        channels,
        render_silence_threshold: 0.0001,
        max_tail_seconds: 120.0,
        show_progress: false,
    }
}

/// 从 MidiDocument 构造 MidiExportData（用于 GPU 临时 MIDI）
fn build_export_data(doc: &MidiDocument, config: &AudioRenderConfig) -> crate::midi::MidiExportData {
    use crate::midi::{
        MidiControlChangeEvent, MidiExportData, MidiExportOptions, MidiKeySignatureEvent,
        MidiNoteEvent, MidiProgramChangeEvent, MidiTempoEvent, MidiTimeSignatureEvent, MidiTrackData,
    };
    use lumino_midi_loader::bpm_to_tempo;

    let mut pc_by_track: std::collections::HashMap<u16, Vec<MidiProgramChangeEvent>> =
        Default::default();
    let mut cc_by_track: std::collections::HashMap<u16, Vec<MidiControlChangeEvent>> =
        Default::default();

    // 仅在非忽略音色时收集 PC/CC
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
            let (program_changes, control_changes) = (
                pc_by_track.get(&track_id).cloned().unwrap_or_default(),
                cc_by_track.get(&track_id).cloned().unwrap_or_default(),
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

/// 使用 GPU 后端从 MidiDocument 渲染到 Sink（内存模式）
pub fn render_audio_gpu_from_document(
    config: &AudioRenderConfig,
    doc: &MidiDocument,
) -> ExportResult<()> {
    use lumino_gpu_synth::GpuSynth;
    use tempfile::NamedTempFile;
    use std::io::Write;

    if config.soundfonts.is_empty() {
        return Err(ExportError::AudioWrite("未指定音色库文件".into()));
    }
    // 校验编码器参数
    if let Err(msg) = config.audio_codec.validate(config.sample_rate, config.audio_bitrate) {
        return Err(ExportError::AudioWrite(msg));
    }

    let synth_config = build_synth_config(config);
    let mut synth = GpuSynth::new(synth_config)
        .map_err(|e| ExportError::AudioWrite(format!("GPU 初始化失败: {e}")))?;

    // 加载音色库（仅首个，多音色库场景可扩展为多次 load）
    let sf_path = &config.soundfonts[0];
    synth
        .load_soundfont(sf_path, 0, 0)
        .map_err(|e| ExportError::AudioWrite(format!("GPU 音色库加载失败 {sf_path:?}: {e}")))?;

    // 构造临时 MIDI 文件
    let export_data = build_export_data(doc, config);
    let midi_bytes = crate::midi::export_midi_to_bytes(&export_data)?;

    let mut tmp = NamedTempFile::new().map_err(|e| ExportError::AudioWrite(format!("创建临时 MIDI 失败: {e}")))?;
    tmp.write_all(&midi_bytes)
        .map_err(|e| ExportError::AudioWrite(format!("写入临时 MIDI 失败: {e}")))?;
    tmp.flush()
        .map_err(|e| ExportError::AudioWrite(format!("刷新临时 MIDI 失败: {e}")))?;
    let tmp_path = tmp.path().to_path_buf();

    // GPU 离线渲染
    let result = synth
        .render_midi_file(&tmp_path)
        .map_err(|e| ExportError::AudioWrite(format!("GPU 渲染失败: {e}")))?;

    // 通过 Sink 写入目标文件（支持 WAV/MP3/FLAC 等）
    write_gpu_result_to_sink(config, &result.samples, result.sample_rate, result.channels)?;

    Ok(())
}

/// 使用 GPU 后端从磁盘 MIDI 文件渲染（流式模式，无 MidiDocument）
pub fn render_audio_gpu_streaming(config: &AudioRenderConfig) -> ExportResult<()> {
    use lumino_gpu_synth::GpuSynth;

    if config.soundfonts.is_empty() {
        return Err(ExportError::AudioWrite("未指定音色库文件".into()));
    }
    if let Err(msg) = config.audio_codec.validate(config.sample_rate, config.audio_bitrate) {
        return Err(ExportError::AudioWrite(msg));
    }

    let synth_config = build_synth_config(config);
    let mut synth = GpuSynth::new(synth_config)
        .map_err(|e| ExportError::AudioWrite(format!("GPU 初始化失败: {e}")))?;

    let sf_path = &config.soundfonts[0];
    synth
        .load_soundfont(sf_path, 0, 0)
        .map_err(|e| ExportError::AudioWrite(format!("GPU 音色库加载失败 {sf_path:?}: {e}")))?;

    let result = synth
        .render_midi_file(&config.midi_path)
        .map_err(|e| ExportError::AudioWrite(format!("GPU 渲染失败: {e}")))?;

    write_gpu_result_to_sink(config, &result.samples, result.sample_rate, result.channels)?;
    Ok(())
}

/// 将 GPU 渲染结果写入 Sink（处理声道数与采样率）
fn write_gpu_result_to_sink(
    config: &AudioRenderConfig,
    samples: &[f32],
    sample_rate: u32,
    channels: u32,
) -> ExportResult<()> {
    // GPU 输出采样率应与 config 一致（SynthConfig 已按 config 构造），若不一致则告警
    if sample_rate != config.sample_rate {
        tracing::warn!(
            "GPU 渲染采样率 {} 与配置 {} 不一致，以渲染结果为准",
            sample_rate,
            config.sample_rate
        );
    }
    // 通道数校验
    let _expected_ch = config.channels.channel_count() as u32;
    if channels != _expected_ch {
        tracing::warn!(
            "GPU 渲染声道 {} 与配置 {} 不一致",
            channels,
            _expected_ch
        );
    }

    let mut sink = create_output_sink(config)?;

    // 分块写入，避免单次过大
    const CHUNK_FRAMES: usize = 4096;
    let ch = channels as usize;
    let mut offset = 0;
    while offset < samples.len() {
        let end = (offset + CHUNK_FRAMES * ch).min(samples.len());
        sink.write_samples(&samples[offset..end])?;
        offset = end;
        // 进度回调（按样本进度估算）
        if let Some(ref cb) = config.progress_callback {
            let pct = offset as f64 / samples.len() as f64;
            cb(format!("GPU 写入 {:.1}%", pct * 100.0), pct);
        }
    }
    sink.finalize()?;
    Ok(())
}

/// 供调用方查询 GPU 后端是否可用
pub fn gpu_backend_available() -> bool {
    is_gpu_available()
}
