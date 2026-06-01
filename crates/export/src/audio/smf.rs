//! MIDI SMF 渲染路径——通过 midly::Smf 结构渲染音频

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use xsynth_core::channel::{ChannelAudioEvent, ChannelEvent, ControlEvent};
use xsynth_core::channel_group::SynthEvent;
use xsynth_core::soundfont::{SampleSoundfont, SoundfontBase};
use xsynth_core::{AudioStreamParams, ChannelCount};

use crate::error::ExportResult;

use super::MidiEventParser;
use super::exporter::AudioExporter;
use super::tempo::TempoMap;
use super::types::AudioExportOptions;
use super::writer::AudioFileWriter;

impl MidiEventParser {
    /// 从已解析的 SMF 对象渲染音频（核心渲染逻辑，复用内存中的 MIDI 数据）
    #[allow(clippy::useless_conversion)]
    pub(super) fn render_smf(
        smf: &midly::Smf,
        exporter: &mut AudioExporter,
        writer: &mut AudioFileWriter,
        progress_callback: Option<&Arc<dyn Fn(f32) + Send + Sync>>,
        cancel_flag: Option<&Arc<AtomicBool>>,
    ) -> ExportResult<()> {
        // 计算总 tick（用于进度回调）
        let total_ticks = Self::calculate_total_ticks(smf);
        let ppqn = match smf.header.timing {
            midly::Timing::Metrical(t) => u16::from(t) as u32,
            midly::Timing::Timecode(_, _) => 480, // 默认值
        };

        // 构建速度映射表（支持 Tempo 变化）
        let tempo_map = TempoMap::from_smf(smf, ppqn);

        let mut current_tick: u64 = 0;
        let mut last_progress = 0.0;

        // 渲染每个轨道
        for track in &smf.tracks {
            let mut track_tick: u64 = 0;
            for event in track {
                // 检查取消标志
                if let Some(cancel) = cancel_flag
                    && cancel.load(Ordering::Relaxed)
                {
                    return Err(
                        (crate::error::ExportError::AudioWrite("导出已取消".to_string())).into(),
                    );
                }

                let delta = u32::from(event.delta) as u64;
                track_tick += delta;

                // 使用速度映射表计算精确时间（支持 Tempo 变化）
                let target_time = tempo_map.tick_to_seconds(track_tick);
                let current_time = tempo_map.tick_to_seconds(current_tick);
                if target_time > current_time {
                    let render_time = target_time - current_time;
                    exporter.render_batch(render_time);

                    // 使用 take_samples 避免 to_vec() 克隆
                    let samples = exporter.take_samples();
                    if !samples.is_empty() {
                        writer.write_samples(&samples)?;
                    }

                    current_tick = track_tick;
                }

                // 发送 MIDI 事件
                match event.kind {
                    midly::TrackEventKind::Midi { channel, message } => {
                        let ch = u8::from(channel) as u32;
                        match message {
                            midly::MidiMessage::NoteOn { key, vel } => {
                                exporter.send_event(SynthEvent::Channel(
                                    ch,
                                    ChannelEvent::Audio(ChannelAudioEvent::NoteOn {
                                        key: u8::from(key),
                                        vel: u8::from(vel),
                                    }),
                                ));
                            }
                            midly::MidiMessage::NoteOff { key, .. } => {
                                exporter.send_event(SynthEvent::Channel(
                                    ch,
                                    ChannelEvent::Audio(ChannelAudioEvent::NoteOff {
                                        key: key.into(),
                                    }),
                                ));
                            }
                            midly::MidiMessage::Controller { controller, value } => {
                                exporter.send_event(SynthEvent::Channel(
                                    ch,
                                    ChannelEvent::Audio(ChannelAudioEvent::Control(
                                        ControlEvent::Raw(u8::from(controller), u8::from(value)),
                                    )),
                                ));
                            }
                            midly::MidiMessage::PitchBend { bend } => {
                                let bend_value = bend.as_int() as f32 / 8192.0;
                                exporter.send_event(SynthEvent::Channel(
                                    ch,
                                    ChannelEvent::Audio(ChannelAudioEvent::Control(
                                        ControlEvent::PitchBendValue(bend_value),
                                    )),
                                ));
                            }
                            midly::MidiMessage::ProgramChange { program } => {
                                exporter.send_event(SynthEvent::Channel(
                                    ch,
                                    ChannelEvent::Audio(ChannelAudioEvent::ProgramChange(
                                        u8::from(program),
                                    )),
                                ));
                            }
                            _ => {}
                        }
                    }
                    midly::TrackEventKind::Meta(_meta) => {
                        // Tempo 事件已在预扫描中处理，此处无需再处理
                    }
                    _ => {}
                }

                // 更新进度（使用 tick 比例估算，速度变化影响不大）
                if let Some(callback) = progress_callback {
                    let progress = (current_tick as f64 / total_ticks as f64 * 100.0).min(99.0);
                    if (progress - last_progress).abs() >= 1.0 {
                        callback(progress as f32);
                        last_progress = progress;
                    }
                }
            }
        }

        Ok(())
    }

    /// 设置导出器 + 文件写入器，并调用核心渲染逻辑
    pub(super) fn setup_and_render(
        smf: &midly::Smf,
        soundfont_path: &Path,
        output_path: &Path,
        options: &AudioExportOptions,
        progress_callback: Option<Arc<dyn Fn(f32) + Send + Sync>>,
        cancel_flag: Option<Arc<AtomicBool>>,
    ) -> ExportResult<()> {
        // 加载音色库
        let audio_params = AudioStreamParams::new(options.sample_rate, ChannelCount::Stereo);
        let soundfont: Arc<dyn SoundfontBase> = Arc::new(
            SampleSoundfont::new(
                soundfont_path,
                audio_params,
                xsynth_core::soundfont::SoundfontInitOptions::default(),
            )
            .map_err(|e| crate::error::ExportError::AudioWrite(format!("音色库加载失败: {}", e)))?,
        );

        // 创建导出器
        let mut exporter = AudioExporter::new(options, soundfont);

        // 创建音频文件写入器
        let mut writer = AudioFileWriter::create(
            output_path,
            options.format,
            options.sample_rate,
            options.channels,
        )?;

        // 核心渲染
        Self::render_smf(
            smf,
            &mut exporter,
            &mut writer,
            progress_callback.as_ref(),
            cancel_flag.as_ref(),
        )?;

        // 完成渲染：将剩余衰减样本直接流式写入 writer
        exporter.finalize(&mut writer)?;

        // 完成文件写入
        writer.finalize()?;

        // 进度 100%
        if let Some(callback) = progress_callback {
            callback(100.0);
        }

        Ok(())
    }

    /// 解析 MIDI 文件并渲染为音频
    #[allow(clippy::useless_conversion)]
    pub fn parse_and_render(
        midi_path: &Path,
        soundfont_path: &Path,
        output_path: &Path,
        options: &AudioExportOptions,
        progress_callback: Option<Arc<dyn Fn(f32) + Send + Sync>>,
        cancel_flag: Option<Arc<AtomicBool>>,
    ) -> ExportResult<()> {
        // 解析 MIDI 文件
        let midi_bytes = std::fs::read(midi_path)
            .map_err(|e| crate::error::ExportError::Io(std::io::Error::other(e)))?;

        let smf = midly::Smf::parse(&midi_bytes)
            .map_err(|e| crate::error::ExportError::MidiParse(format!("MIDI 解析失败: {}", e)))?;

        Self::setup_and_render(
            &smf,
            soundfont_path,
            output_path,
            options,
            progress_callback,
            cancel_flag,
        )
    }

    /// 计算 MIDI 总 tick 数
    fn calculate_total_ticks(smf: &midly::Smf) -> u64 {
        let mut max_tick: u64 = 0;
        for track in &smf.tracks {
            let mut tick: u64 = 0;
            for event in track {
                tick += u32::from(event.delta) as u64;
                if tick > max_tick {
                    max_tick = tick;
                }
            }
        }
        max_tick
    }
}
