//! 音频导出功能
//!
//! 使用 xsynth-core 将 MIDI 文件渲染为 WAV/FLAC 音频文件。

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use xsynth_core::channel::{ChannelAudioEvent, ChannelConfigEvent, ChannelEvent, ControlEvent};
use xsynth_core::channel_group::{ChannelGroup, ChannelGroupConfig, SynthEvent};
use xsynth_core::effects::VolumeLimiter;
use xsynth_core::soundfont::{SampleSoundfont, SoundfontBase};
use xsynth_core::{AudioPipe, AudioStreamParams, ChannelCount};

use crate::error::{ExportError, ExportResult};

/// 音频导出格式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AudioFormat {
    #[default]
    WAV,
    FLAC,
}

impl std::fmt::Display for AudioFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioFormat::WAV => write!(f, "WAV"),
            AudioFormat::FLAC => write!(f, "FLAC"),
        }
    }
}

/// 音频通道数
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AudioChannels {
    Mono,
    #[default]
    Stereo,
}

impl AudioChannels {
    pub fn count(&self) -> u16 {
        match self {
            AudioChannels::Mono => 1,
            AudioChannels::Stereo => 2,
        }
    }
}

impl std::fmt::Display for AudioChannels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioChannels::Mono => write!(f, "单声道"),
            AudioChannels::Stereo => write!(f, "立体声"),
        }
    }
}

/// 多线程选项
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThreadingOption {
    None,
    #[default]
    Auto,
    Manual(u32),
}

/// 插值算法
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Interpolation {
    None,
    #[default]
    Linear,
}

/// 音频导出选项
#[derive(Debug, Clone)]
pub struct AudioExportOptions {
    /// 采样率 (Hz)
    pub sample_rate: u32,
    /// 音频通道数
    pub channels: AudioChannels,
    /// 每通道层数限制 (0 = 无限制)
    pub layers: u32,
    /// 通道多线程选项
    pub channel_threading: ThreadingOption,
    /// 按键多线程选项
    pub key_threading: ThreadingOption,
    /// 应用限制器防削波
    pub apply_limiter: bool,
    /// 禁用淡出（可能爆音）
    pub disable_fade_out: bool,
    /// 线性包络
    pub linear_envelope: bool,
    /// 插值算法
    pub interpolation: Interpolation,
    /// 输出格式
    pub format: AudioFormat,
}

impl Default for AudioExportOptions {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            channels: AudioChannels::default(),
            layers: 32,
            channel_threading: ThreadingOption::default(),
            key_threading: ThreadingOption::default(),
            apply_limiter: true,
            disable_fade_out: false,
            linear_envelope: false,
            interpolation: Interpolation::default(),
            format: AudioFormat::default(),
        }
    }
}

/// 音频导出进度回调类型
pub type ProgressCallback = Box<dyn Fn(f32) + Send + Sync>;

/// 音频导出器
pub struct AudioExporter {
    channel_group: ChannelGroup,
    limiter: Option<VolumeLimiter>,
    sample_rate: u32,
    channels: AudioChannels,
    output_vec: Vec<f32>,
    missed_samples: f64,
}

impl AudioExporter {
    /// 创建新的音频导出器
    pub fn new(options: &AudioExportOptions, soundfont: Arc<dyn SoundfontBase>) -> Self {
        let audio_params = AudioStreamParams::new(options.sample_rate, ChannelCount::Stereo);

        let group_options = ChannelGroupConfig {
            channel_init_options: xsynth_core::channel::ChannelInitOptions::default(),
            format: xsynth_core::channel_group::SynthFormat::Midi,
            audio_params,
            parallelism: xsynth_core::channel_group::ParallelismOptions::default(),
        };

        let channel_group = ChannelGroup::new(group_options);

        let limiter = if options.apply_limiter {
            Some(VolumeLimiter::new(options.channels.count()))
        } else {
            None
        };

        let mut exporter = Self {
            channel_group,
            limiter,
            sample_rate: options.sample_rate,
            channels: options.channels,
            output_vec: Vec::new(),
            missed_samples: 0.0,
        };

        // 设置音色库
        exporter.channel_group.send_event(SynthEvent::AllChannels(
            ChannelEvent::Config(ChannelConfigEvent::SetSoundfonts(vec![soundfont])),
        ));

        // 设置层数限制
        exporter.channel_group.send_event(SynthEvent::AllChannels(
            ChannelEvent::Config(ChannelConfigEvent::SetLayerCount(Some(
                options.layers as usize,
            ))),
        ));

        exporter
    }

    /// 发送合成事件
    pub fn send_event(&mut self, event: SynthEvent) {
        self.channel_group.send_event(event);
    }

    /// 渲染指定时间的音频样本
    pub fn render_batch(&mut self, event_time: f64) {
        if event_time > 10.0 {
            let mut remaining_time = event_time;
            loop {
                if remaining_time > 10.0 {
                    self.render_batch(10.0);
                    remaining_time -= 10.0;
                } else {
                    self.render_batch(remaining_time);
                    break;
                }
            }
        } else {
            let samples = self.sample_rate as f64 * event_time + self.missed_samples;
            self.missed_samples = samples % 1.0;
            let samples = samples as usize * self.channels.count() as usize;

            self.output_vec.resize(samples, 0.0);
            self.channel_group.read_samples(&mut self.output_vec);

            if let Some(limiter) = &mut self.limiter {
                limiter.limit(&mut self.output_vec);
            }
        }
    }

    /// 获取当前渲染的样本
    pub fn get_samples(&self) -> &[f32] {
        &self.output_vec
    }

    /// 清空样本缓冲区
    pub fn clear_samples(&mut self) {
        self.output_vec.clear();
    }

    /// 完成渲染，返回剩余样本
    pub fn finalize(&mut self) -> Vec<f32> {
        let mut all_samples = Vec::new();

        loop {
            self.output_vec
                .resize(self.sample_rate as usize, 0.0);
            self.channel_group.read_samples(&mut self.output_vec);

            if let Some(limiter) = &mut self.limiter {
                limiter.limit(&mut self.output_vec);
            }

            let mut is_empty = true;
            for s in &self.output_vec {
                if *s > 0.0001 || *s < -0.0001 {
                    is_empty = false;
                    break;
                }
            }

            if is_empty {
                break;
            }

            all_samples.extend_from_slice(&self.output_vec);
        }

        all_samples
    }

    /// 获取活跃 voice 数量
    pub fn voice_count(&self) -> u64 {
        self.channel_group.voice_count()
    }
}

/// 音频文件写入器
pub enum AudioFileWriter {
    WAV(hound::WavWriter<std::io::BufWriter<std::fs::File>>),
    FLAC {
        path: std::path::PathBuf,
        sample_rate: u32,
        channels: u16,
        samples: Vec<i16>,
    },
}

impl AudioFileWriter {
    /// 创建新的音频文件写入器
    pub fn create(
        path: &Path,
        format: AudioFormat,
        sample_rate: u32,
        channels: AudioChannels,
    ) -> ExportResult<Self> {
        match format {
            AudioFormat::WAV => {
                let spec = hound::WavSpec {
                    channels: channels.count(),
                    sample_rate,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                };
                let writer = hound::WavWriter::create(path, spec)
                    .map_err(|e| ExportError::AudioWrite(e.to_string()))?;
                Ok(Self::WAV(writer))
            }
            AudioFormat::FLAC => {
                // FLAC: 收集所有样本，最后一次性编码
                Ok(Self::FLAC {
                    path: path.to_path_buf(),
                    sample_rate,
                    channels: channels.count(),
                    samples: Vec::new(),
                })
            }
        }
    }

    /// 写入样本
    pub fn write_samples(&mut self, samples: &[f32]) -> ExportResult<()> {
        // 将 f32 样本转换为 i16
        let i16_samples: Vec<i16> = samples
            .iter()
            .map(|&s| {
                let clamped = s.clamp(-1.0, 1.0);
                (clamped * 32767.0) as i16
            })
            .collect();

        match self {
            Self::WAV(writer) => {
                for &sample in &i16_samples {
                    writer
                        .write_sample(sample)
                        .map_err(|e| ExportError::AudioWrite(e.to_string()))?;
                }
                Ok(())
            }
            Self::FLAC { samples, .. } => {
                samples.extend_from_slice(&i16_samples);
                Ok(())
            }
        }
    }

    /// 完成写入
    pub fn finalize(self) -> ExportResult<()> {
        match self {
            Self::WAV(writer) => {
                writer
                    .finalize()
                    .map_err(|e| ExportError::AudioWrite(e.to_string()))?;
                Ok(())
            }
            Self::FLAC {
                path,
                sample_rate,
                channels,
                samples,
            } => {
                // 将 i16 转换为 f32 (范围 -1.0 到 1.0)
                let f32_samples: Vec<f32> = samples
                    .iter()
                    .map(|&s| s as f32 / 32767.0)
                    .collect();

                // 使用 flac-encoder 编码
                let flac_data = flac_encoder::FlacBuilder::from_interleaved(
                    &f32_samples,
                    channels as usize,
                    sample_rate,
                )
                .build()
                .map_err(|e| ExportError::AudioWrite(format!("FLAC 编码失败: {:?}", e)))?;

                std::fs::write(&path, &flac_data)
                    .map_err(|e| ExportError::AudioWrite(format!("FLAC 文件写入失败: {}", e)))?;

                Ok(())
            }
        }
    }
}

/// MIDI 事件解析器
pub struct MidiEventParser;

impl MidiEventParser {
    /// 解析 MIDI 文件并渲染为音频
    pub fn parse_and_render(
        midi_path: &Path,
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
            .map_err(|e| ExportError::AudioWrite(format!("音色库加载失败: {}", e)))?,
        );

        // 创建导出器
        let mut exporter = AudioExporter::new(options, soundfont);

        // 创建音频文件写入器
        let mut writer = AudioFileWriter::create(output_path, options.format, options.sample_rate, options.channels)?;

        // 解析 MIDI 文件
        let midi_bytes = std::fs::read(midi_path)
            .map_err(|e| ExportError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        let smf = midly::Smf::parse(&midi_bytes)
            .map_err(|e| ExportError::MidiParse(format!("MIDI 解析失败: {}", e)))?;

        // 计算总时长（用于进度回调）
        let total_ticks = Self::calculate_total_ticks(&smf);
        let ppqn = match smf.header.timing {
            midly::Timing::Metrical(t) => u16::from(t) as u32,
            midly::Timing::Timecode(_, _) => 480, // 默认值
        };
        let ticks_per_second = ppqn as f64 * 120.0 / 60.0; // 假设 120 BPM
        let _total_seconds = total_ticks as f64 / ticks_per_second;

        let mut current_tick: u64 = 0;
        let mut last_progress = 0.0;

        // 渲染每个轨道
        for track in &smf.tracks {
            let mut track_tick: u64 = 0;
            for event in track {
                // 检查取消标志
                if let Some(ref cancel) = cancel_flag {
                    if cancel.load(Ordering::Relaxed) {
                        return Err(ExportError::AudioWrite("导出已取消".to_string()));
                    }
                }

                let delta = u32::from(event.delta) as u64;
                track_tick += delta;

                // 渲染到当前时间
                let target_time = track_tick as f64 / ticks_per_second;
                let current_time = current_tick as f64 / ticks_per_second;
                if target_time > current_time {
                    let render_time = target_time - current_time;
                    exporter.render_batch(render_time);

                    // 写入样本
                    let samples = exporter.get_samples().to_vec();
                    writer.write_samples(&samples)?;
                    exporter.clear_samples();

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
                                        key: u8::from(key),
                                    }),
                                ));
                            }
                            midly::MidiMessage::Controller { controller, value } => {
                                exporter.send_event(SynthEvent::Channel(
                                    ch,
                                    ChannelEvent::Audio(ChannelAudioEvent::Control(
                                        ControlEvent::Raw(
                                            u8::from(controller),
                                            u8::from(value),
                                        ),
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
                    midly::TrackEventKind::Meta(meta) => {
                        // 处理速度变化等元事件
                        if let midly::MetaMessage::Tempo(_tempo) = meta {
                            // 速度变化事件，暂时忽略
                        }
                    }
                    _ => {}
                }

                // 更新进度
                if let Some(ref callback) = progress_callback {
                    let progress = (current_tick as f64 / total_ticks as f64 * 100.0).min(99.0);
                    if (progress - last_progress).abs() >= 1.0 {
                        callback(progress as f32);
                        last_progress = progress;
                    }
                }
            }
        }

        // 完成渲染
        let final_samples = exporter.finalize();
        if !final_samples.is_empty() {
            writer.write_samples(&final_samples)?;
        }

        // 完成文件写入
        writer.finalize()?;

        // 进度 100%
        if let Some(callback) = progress_callback {
            callback(100.0);
        }

        Ok(())
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

/// 导出音频文件
///
/// # 参数
/// - `midi_path`: MIDI 文件路径
/// - `soundfont_path`: SF2 音色库路径
/// - `output_path`: 输出音频文件路径
/// - `options`: 导出选项
/// - `progress_callback`: 进度回调 (0.0 - 100.0)
/// - `cancel_flag`: 取消标志
///
/// # 返回
/// 成功返回 `Ok(())`，失败返回 `Err(ExportError)`
pub fn export_audio(
    midi_path: &Path,
    soundfont_path: &Path,
    output_path: &Path,
    options: &AudioExportOptions,
    progress_callback: Option<Arc<dyn Fn(f32) + Send + Sync>>,
    cancel_flag: Option<Arc<AtomicBool>>,
) -> ExportResult<()> {
    // 验证输入文件
    if !midi_path.exists() {
        return Err(ExportError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("MIDI 文件不存在: {:?}", midi_path),
        )));
    }

    if !soundfont_path.exists() {
        return Err(ExportError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("音色库文件不存在: {:?}", soundfont_path),
        )));
    }

    // 创建输出目录
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ExportError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
    }

    tracing::info!(
        "开始音频导出: MIDI={:?}, SF2={:?}, 输出={:?}, 格式={}, 采样率={}Hz",
        midi_path,
        soundfont_path,
        output_path,
        options.format,
        options.sample_rate
    );

    let start = std::time::Instant::now();

    MidiEventParser::parse_and_render(
        midi_path,
        soundfont_path,
        output_path,
        options,
        progress_callback,
        cancel_flag,
    )?;

    let elapsed = start.elapsed();
    tracing::info!("音频导出完成，耗时: {:.2} 秒", elapsed.as_secs_f64());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(options.layers, 32);
        assert!(options.apply_limiter);
        assert_eq!(options.format, AudioFormat::WAV);
    }
}
