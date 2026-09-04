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

    // xsynth 每 key 32 复音，与 lumino-export 的 layer_limit 语义一致；GPU 原硬编码 4 导致同音高密集时过度抢占
    // 0/None 表示无限制，需保持 0 而非 max(4)
    let max_voices_per_key = match config.layer_limit {
        None | Some(0) => 0,
        Some(n) => n.max(4),
    };
    SynthConfig {
        sample_rate: config.sample_rate,
        max_voices: config.layer_limit.unwrap_or(0),
        max_voices_per_key,
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
fn build_export_data(
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

fn check_control(config: &AudioRenderConfig) -> ExportResult<()> {
    if let Some(ctrl) = &config.control {
        ctrl.wait_if_paused();
        ctrl.check_abort()?;
    }
    Ok(())
}

/// 使用 GPU 后端从 MidiDocument 渲染到 Sink（内存模式）
pub fn render_audio_gpu_from_document(
    config: &AudioRenderConfig,
    doc: &MidiDocument,
) -> ExportResult<()> {
    use lumino_gpu_synth::GpuSynth;
    use std::io::Write;
    use tempfile::NamedTempFile;

    check_control(config)?;

    let report = |msg: &str, pct: f64| {
        if let Some(ref cb) = config.progress_callback {
            cb(msg.to_string(), pct);
        }
    };

    if config.soundfonts.is_empty() {
        return Err(ExportError::AudioWrite(
            "未指定音色库文件，请先在 音色库(SF2) 中选择 .sf2 文件".into(),
        ));
    }
    // 校验编码器参数
    if let Err(msg) = config
        .audio_codec
        .validate(config.sample_rate, config.audio_bitrate)
    {
        return Err(ExportError::AudioWrite(msg));
    }
    // 检查音色库文件存在与格式（SFZ 会直接 panic，需前置拦截）
    let sf_path = &config.soundfonts[0];
    if !sf_path.exists() {
        return Err(ExportError::AudioWrite(format!(
            "音色库文件不存在: {:?}，请检查路径或重新选择",
            sf_path
        )));
    }
    if let Some(ext) = sf_path.extension().and_then(|s| s.to_str())
        && !ext.eq_ignore_ascii_case("sf2")
        && !ext.eq_ignore_ascii_case("sfz")
    {
        return Err(ExportError::AudioWrite(format!(
            "不支持的音色库格式 {:?}：仅支持 .sf2/.sfz",
            sf_path
        )));
    }
    // 仅对 SF2 校验 RIFF 头，SFZ 为文本
    if let Some(ext) = sf_path.extension().and_then(|s| s.to_str())
        && ext.eq_ignore_ascii_case("sf2")
        && let Ok(mut f) = std::fs::File::open(sf_path)
    {
        use std::io::Read;
        let mut header = [0u8; 4];
        if f.read_exact(&mut header).is_ok() && header != *b"RIFF" {
            return Err(ExportError::AudioWrite(format!(
                "音色库不是合法的 SF2 {:?}：头应为 RIFF，实际 {:02X?}",
                sf_path, header
            )));
        }
    }

    report("GPU 初始化中...", 0.05);
    check_control(config)?;
    let synth_config = build_synth_config(config);
    let mut synth = GpuSynth::new(synth_config).map_err(|e| {
        ExportError::AudioWrite(format!(
            "GPU 初始化失败（可能无可用 Vulkan/Metal 适配器）: {e}"
        ))
    })?;

    report("GPU 加载音色库...", 0.10);
    check_control(config)?;
    synth
        .load_soundfont(sf_path, 0, 0)
        .map_err(|e| ExportError::AudioWrite(format!("GPU 音色库加载失败 {sf_path:?}: {e}")))?;

    report("GPU 导出临时 MIDI...", 0.15);
    check_control(config)?;
    // 空文档直接报错，交由上层回退到文件模式
    let total_notes: usize = doc.notes.iter().map(|v| v.len()).sum();
    if total_notes == 0 && doc.control_events.is_empty() {
        return Err(ExportError::AudioWrite(
            "MIDI 文档中没有可渲染的事件（0 notes），请检查 MIDI 是否已加载".into(),
        ));
    }
    // 构造临时 MIDI 文件
    let export_data = build_export_data(doc, config);
    let midi_bytes = crate::midi::export_midi_to_bytes(&export_data)?;

    let mut tmp = NamedTempFile::new()
        .map_err(|e| ExportError::AudioWrite(format!("创建临时 MIDI 失败: {e}")))?;
    tmp.write_all(&midi_bytes)
        .map_err(|e| ExportError::AudioWrite(format!("写入临时 MIDI 失败: {e}")))?;
    tmp.flush()
        .map_err(|e| ExportError::AudioWrite(format!("刷新临时 MIDI 失败: {e}")))?;
    let tmp_path = tmp.path().to_path_buf();

    report("GPU 渲染中（可能耗时，黑 MIDI 请耐心）...", 0.20);
    check_control(config)?;
    // GPU 离线渲染（阻塞，期间无细粒度进度，靠最终写入阶段推进到 1.0）
    // 粗粒度暂停/中止：渲染前检查，渲染本身为长时间阻塞，暂停会在下次检查生效
    let result = synth
        .render_midi_file(&tmp_path)
        .map_err(|e| ExportError::AudioWrite(format!("GPU 渲染失败: {e}")))?;
    check_control(config)?;

    report("GPU 写入输出...", 0.85);
    // 通过 Sink 写入目标文件（支持 WAV/MP3/FLAC 等）
    write_gpu_result_to_sink(config, &result.samples, result.sample_rate, result.channels)?;

    report("GPU 完成", 1.0);
    Ok(())
}

/// 使用 GPU 后端从磁盘 MIDI 文件渲染（流式模式，无 MidiDocument）
pub fn render_audio_gpu_streaming(config: &AudioRenderConfig) -> ExportResult<()> {
    use lumino_gpu_synth::GpuSynth;

    check_control(config)?;

    let report = |msg: &str, pct: f64| {
        if let Some(ref cb) = config.progress_callback {
            cb(msg.to_string(), pct);
        }
    };

    if config.soundfonts.is_empty() {
        return Err(ExportError::AudioWrite(
            "未指定音色库文件，请先在 音色库(SF2) 中选择 .sf2 文件".into(),
        ));
    }
    if let Err(msg) = config
        .audio_codec
        .validate(config.sample_rate, config.audio_bitrate)
    {
        return Err(ExportError::AudioWrite(msg));
    }
    let sf_path = &config.soundfonts[0];
    if !sf_path.exists() {
        return Err(ExportError::AudioWrite(format!(
            "音色库文件不存在: {:?}",
            sf_path
        )));
    }
    if let Some(ext) = sf_path.extension().and_then(|s| s.to_str())
        && !ext.eq_ignore_ascii_case("sf2")
        && !ext.eq_ignore_ascii_case("sfz")
    {
        return Err(ExportError::AudioWrite(format!(
            "不支持的音色库格式 {:?}：仅支持 .sf2/.sfz",
            sf_path
        )));
    }
    if let Some(ext) = sf_path.extension().and_then(|s| s.to_str())
        && ext.eq_ignore_ascii_case("sf2")
        && let Ok(mut f) = std::fs::File::open(sf_path)
    {
        use std::io::Read;
        let mut header = [0u8; 4];
        if f.read_exact(&mut header).is_ok() && header != *b"RIFF" {
            return Err(ExportError::AudioWrite(format!(
                "音色库不是合法的 SF2 {:?}：头应为 RIFF，实际 {:02X?}",
                sf_path, header
            )));
        }
    }
    if !config.midi_path.exists() {
        return Err(ExportError::AudioWrite(format!(
            "MIDI 文件不存在: {:?}",
            config.midi_path
        )));
    }

    report("GPU 初始化中...", 0.05);
    check_control(config)?;
    let synth_config = build_synth_config(config);
    let mut synth = GpuSynth::new(synth_config)
        .map_err(|e| ExportError::AudioWrite(format!("GPU 初始化失败: {e}")))?;

    report("GPU 加载音色库...", 0.10);
    check_control(config)?;
    synth
        .load_soundfont(sf_path, 0, 0)
        .map_err(|e| ExportError::AudioWrite(format!("GPU 音色库加载失败 {sf_path:?}: {e}")))?;

    report("GPU 渲染中...", 0.20);
    check_control(config)?;
    let result = synth
        .render_midi_file(&config.midi_path)
        .map_err(|e| ExportError::AudioWrite(format!("GPU 渲染失败: {e}")))?;
    check_control(config)?;

    report("GPU 写入输出...", 0.85);
    write_gpu_result_to_sink(config, &result.samples, result.sample_rate, result.channels)?;
    report("GPU 完成", 1.0);
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
        tracing::warn!("GPU 渲染声道 {} 与配置 {} 不一致", channels, _expected_ch);
    }

    let mut sink = create_output_sink(config)?;

    // 分块写入，避免单次过大；进度从 0.85 映射到 1.0
    const CHUNK_FRAMES: usize = 4096;
    let ch = channels as usize;
    let mut offset = 0;
    while offset < samples.len() {
        check_control(config)?;
        let end = (offset + CHUNK_FRAMES * ch).min(samples.len());
        sink.write_samples(&samples[offset..end])?;
        offset = end;
        // 进度回调（按样本进度估算 0.85→1.0）
        if let Some(ref cb) = config.progress_callback {
            let inner = offset as f64 / samples.len().max(1) as f64;
            let pct = 0.85 + inner * 0.15;
            cb(format!("GPU 写入 {:.1}%", inner * 100.0), pct);
        }
    }
    sink.finalize()?;
    Ok(())
}

/// 供调用方查询 GPU 后端是否可用
pub fn gpu_backend_available() -> bool {
    is_gpu_available()
}
