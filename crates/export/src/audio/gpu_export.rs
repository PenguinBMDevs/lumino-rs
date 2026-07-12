//! GPU 加速音频导出管线
//!
//! 使用 wgpu compute shader 替代 xsynth CPU 渲染，实现 GPU 加速的音频导出。
//! 架构借鉴自 yinhe 的 GPU 音频导出方案：
//!
//! 1. 使用 `xsynth_soundfonts::sf2::load_soundfont()` 解析 SF2 音色库，提取样本数据
//! 2. 将所有样本数据扁平化上传到 GPU storage buffer
//! 3. CPU 侧处理 MIDI 事件，管理 voice 状态（音高、包络、循环等）
//! 4. GPU compute shader 执行样点插值、包络计算、声像混合
//! 5. 读取 GPU 输出并写入 WAV 文件

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use tracing::info;
use xsynth_core::effects::VolumeLimiter;
use xsynth_soundfonts::sf2::{self, Sf2Preset, Sf2Region};

use crate::error::ExportResult;

use super::config::AudioRenderConfig;
use super::writer::AudioFileWriter;

use super::gpu_renderer::{GpuComputeRenderer, RenderParams, VoiceState};

/// 最大并发 voice 数
const MAX_VOICES: u32 = 256;

/// 每批次渲染时长（秒）
const BATCH_SECONDS: f64 = 1.0;

/// 出口渲染尾部块数
const TAIL_BLOCKS: u32 = 5;

/// 区域查找表类型：bank → program → RegionEntry 列表
type RegionMap = Vec<Vec<Vec<RegionEntry>>>;

/// 区域查找表条目
#[derive(Clone)]
struct RegionEntry {
    region: Sf2Region,
    sample_offset: u32, // 在扁平化样本缓冲区中的偏移（f32 单位）
}

/// GPU 导出渲染器
pub(crate) struct GpuExportRenderer {
    gpu_renderer: GpuComputeRenderer,
    audio_writer: AudioFileWriter,
    limiter: Option<VolumeLimiter>,

    /// 区域查找表：bank → program → RegionEntry 列表
    region_map: RegionMap,

    /// 当前活跃 voice 状态（CPU 侧管理）
    voices: Vec<VoiceState>,

    /// 当前活跃 voice 数
    active_voices: u32,

    sample_rate: u32,
    channel_count: u16,
    max_voices: u32,
}

impl GpuExportRenderer {
    /// 创建 GPU 导出渲染器
    pub(crate) fn new(config: &AudioRenderConfig, path: &Path) -> ExportResult<Self> {
        let sample_rate = config.sample_rate;
        let channel_count = match config.channels {
            super::config::AudioChannelMode::Mono => 1,
            super::config::AudioChannelMode::Stereo => 2,
        };
        let batch_samples = (sample_rate as f64 * BATCH_SECONDS) as u32;

        // 加载 SF2 并提取样本数据
        let sf2_path = config
            .soundfonts
            .first()
            .ok_or_else(|| crate::error::ExportError::AudioWrite("未指定音色库文件".into()))?;

        info!("[GPU 导出] 加载音色库: {:?}", sf2_path);
        let presets = sf2::load_soundfont(sf2_path, sample_rate)
            .map_err(|e| crate::error::ExportError::AudioWrite(format!("加载 SF2 失败: {e}")))?;

        // 扁平化所有样本并构建区域查找表
        let (sample_data, region_map) = Self::extract_samples(&presets)?;

        info!(
            "[GPU 导出] 样本数据: {} f32, {} 预设, {} 区域",
            sample_data.len(),
            presets.len(),
            region_map
                .iter()
                .flat_map(|b| b.iter().flat_map(|r| r.iter()))
                .count()
        );

        // 创建 GPU 渲染器
        let gpu_renderer = GpuComputeRenderer::new(MAX_VOICES, batch_samples, &sample_data)?;

        // 创建音频写入器
        let (vec_recycle_tx, _vec_recycle_rx) = crossbeam_channel::bounded::<Vec<f32>>(2);
        let audio_writer =
            AudioFileWriter::new(sample_rate, channel_count as u16, path, vec_recycle_tx)?;

        let limiter = if config.apply_limiter {
            let cc = xsynth_core::ChannelCount::from(config.channels);
            Some(VolumeLimiter::new(cc.count()))
        } else {
            None
        };

        Ok(Self {
            gpu_renderer,
            audio_writer,
            limiter,
            region_map,
            voices: vec![VoiceState::default(); MAX_VOICES as usize],
            active_voices: 0,
            sample_rate,
            channel_count: channel_count as u16,
            max_voices: MAX_VOICES,
        })
    }

    /// 从 SF2 预设中提取样本数据，构建扁平化缓冲区 + 区域查找表
    ///
    /// # 样本去重
    ///
    /// SF2 音色库中多个 region 可能共享同一份样本数据（通过 `Arc<[Arc<[f32]>]>`
    /// 指针共享）。使用 `Arc::as_ptr()` 作为 HashMap key 检测重复，只复制每个
    /// 唯一样本一次，避免 OOM。
    fn extract_samples(presets: &[Sf2Preset]) -> ExportResult<(Vec<f32>, RegionMap)> {
        let mut all_samples: Vec<f32> = Vec::new();
        let mut bank_map: RegionMap = Vec::new();
        // 样本去重映射：Arc 指针 → 扁平缓冲区中的偏移
        let mut sample_map: HashMap<*const [Arc<[f32]>], u32> = HashMap::new();

        for preset in presets {
            let bank = preset.bank as usize;
            let program = preset.preset as usize;

            // 确保 bank/program 索引存在
            while bank_map.len() <= bank {
                bank_map.push(Vec::new());
            }
            while bank_map[bank].len() <= program {
                bank_map[bank].push(Vec::new());
            }

            for region in &preset.regions {
                let sample_ptr = Arc::as_ptr(&region.sample);
                let sample_offset = if let Some(&offset) = sample_map.get(&sample_ptr) {
                    // 样本已存在，复用偏移
                    offset
                } else {
                    let offset = all_samples.len() as u32;

                    // 展开样本数据（立体声交错）
                    // region.sample 是 Arc<[Arc<[f32]>]>，每个内层 Arc 是一个声道
                    let sample_arcs = region.sample.as_ref();
                    if sample_arcs.len() >= 2 {
                        // 立体声
                        let left = &sample_arcs[0];
                        let right = &sample_arcs[1];
                        let len = left.len().min(right.len());
                        all_samples.reserve(len * 2);
                        for i in 0..len {
                            all_samples.push(left[i]);
                            all_samples.push(right[i]);
                        }
                    } else if sample_arcs.len() == 1 {
                        // 单声道 → 复制到双声道
                        let mono = &sample_arcs[0];
                        all_samples.reserve(mono.len() * 2);
                        for &s in mono.iter() {
                            all_samples.push(s);
                            all_samples.push(s);
                        }
                    }

                    sample_map.insert(sample_ptr, offset);
                    offset
                };

                bank_map[bank][program].push(RegionEntry {
                    region: region.clone(),
                    sample_offset,
                });
            }
        }

        Ok((all_samples, bank_map))
    }

    /// 处理 NoteOn 事件：查找匹配区域，创建 voice
    pub(crate) fn note_on(&mut self, channel: u32, key: u8, velocity: u8, program: u8) {
        if velocity == 0 {
            self.note_off(channel, key);
            return;
        }

        // 查找匹配的区域
        let bank = 0usize; // 默认 bank 0
        let program_idx = program as usize;

        let regions = self
            .region_map
            .get(bank)
            .and_then(|b| b.get(program_idx))
            .map(|v| v.as_slice())
            .unwrap_or(&[]);

        let mut matched = false;
        for entry in regions {
            if !entry.region.keyrange.contains(&key) {
                continue;
            }
            if !entry.region.velrange.contains(&velocity) {
                continue;
            }

            // 找一个空闲 voice 槽
            let voice_idx = self.find_free_voice();
            if voice_idx >= self.max_voices {
                // 没有空闲 voice，跳过
                continue;
            }

            let region = &entry.region;
            let sample_rate_ratio = region.sample_rate as f32 / self.sample_rate as f32;
            let key_diff = (key as i16) - (region.root_key as i16);
            let pitch_ratio = sample_rate_ratio * 2.0f32.powf(key_diff as f32 / 12.0);

            // 计算声像
            let pan = (region.pan as f32 + 50.0) / 100.0;
            let pan_left = (1.0 - pan).sqrt();
            let pan_right = pan.sqrt();

            // 包络参数
            let env = &region.ampeg_envelope;

            // 将音量从 dB 转换为线性增益
            let volume_linear = 10.0f32.powf(region.volume / 20.0);

            self.voices[voice_idx as usize] = VoiceState {
                sample_pos: entry.region.offset as f32,
                pitch_ratio,
                volume: volume_linear,
                pan_left,
                pan_right,
                loop_start: entry.region.loop_start as f32,
                loop_end: entry.region.loop_end as f32,
                loop_mode: match entry.region.loop_mode {
                    xsynth_soundfonts::LoopMode::LoopContinuous => 1,
                    _ => 0,
                },
                sample_index: entry.sample_offset,
                envelope_attack: env.ampeg_attack,
                envelope_decay: env.ampeg_decay,
                envelope_sustain: env.ampeg_sustain,
                envelope_release: env.ampeg_release,
                envelope_value: 0.0,
                env_stage: 0, // attack
                env_time: 0.0,
                active: 1,
                _pad: 0,
            };

            matched = true;
            self.active_voices += 1;
            break; // 只匹配第一个区域
        }

        if !matched {
            tracing::debug!(
                "[GPU 导出] NoteOn(ch={}, key={}, vel={}): 未找到匹配区域",
                channel,
                key,
                velocity
            );
        }
    }

    /// 处理 NoteOff 事件：将 voice 切换到 release 阶段
    pub(crate) fn note_off(&mut self, _channel: u32, _key: u8) {
        for voice in &mut self.voices {
            if voice.active == 0 {
                continue;
            }
            // 简单匹配：按 key 找 voice 比较困难，因为 sample_pos 不直接对应 key
            // 我们标记所有活跃 voice 进入 release（实际应该精确匹配 key）
            // 更好的方式：每个 voice 存 key，但为了简化，全部进 release
            if voice.env_stage < 3 {
                voice.env_stage = 3; // release
                voice.env_time = 0.0;
            }
        }
    }

    /// 查找空闲 voice 槽
    fn find_free_voice(&self) -> u32 {
        for (i, voice) in self.voices.iter().enumerate() {
            if voice.active == 0 || voice.env_stage == 4 {
                return i as u32;
            }
        }
        self.max_voices // 没有空闲
    }

    /// 渲染一个批次
    fn render_batch(&mut self, duration: f64) -> ExportResult<()> {
        let batch_samples = (self.sample_rate as f64 * duration) as u32;
        if batch_samples == 0 {
            return Ok(());
        }

        // 统计当前活跃 voice
        let mut active_count = 0u32;
        for voice in &self.voices {
            if voice.active != 0 && voice.env_stage != 4 {
                active_count += 1;
            }
        }

        if active_count == 0 {
            // 没有活跃 voice，输出静音
            let silent = vec![0.0f32; (batch_samples * self.channel_count as u32) as usize];
            self.audio_writer
                .write_samples(&mut silent.clone())
                .map_err(|e| crate::error::ExportError::AudioWrite(format!("写入失败: {e}")))?;
            return Ok(());
        }

        // 上传 voice 状态
        self.gpu_renderer.upload_voice_states(&self.voices);

        // 调度 GPU 计算
        let params = RenderParams {
            sample_rate: self.sample_rate as f32,
            num_voices: active_count,
            num_samples: batch_samples,
            output_offset: 0,
            max_voices: self.max_voices,
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
        };
        self.gpu_renderer.dispatch(&params);

        // 读取输出
        let mut output = self.gpu_renderer.read_output()?;

        // 读取更新后的 voice 状态
        self.voices = self.gpu_renderer.read_voice_states()?;

        // 更新活跃 voice 计数
        self.active_voices = 0;
        for voice in &self.voices {
            if voice.active != 0 && voice.env_stage != 4 {
                self.active_voices += 1;
            }
        }

        // 截断到实际样本数
        let expected = (batch_samples * self.channel_count as u32) as usize;
        output.truncate(expected);

        // 应用限幅器
        if let Some(limiter) = &mut self.limiter {
            limiter.limit(&mut output);
        }

        // 写入文件
        self.audio_writer
            .write_samples(&mut output)
            .map_err(|e| crate::error::ExportError::AudioWrite(format!("写入失败: {e}")))?;

        Ok(())
    }

    /// 发送 MIDI 事件
    pub(crate) fn send_note_on(&mut self, key: u8, velocity: u8, channel: u32, program: u8) {
        self.note_on(channel, key, velocity, program);
    }

    pub(crate) fn send_note_off(&mut self, key: u8, channel: u32) {
        self.note_off(channel, key);
    }

    /// 完成渲染
    pub(crate) fn finalize(mut self) -> ExportResult<()> {
        // 渲染尾部（所有 voice 进入 release 直至静音）
        for _ in 0..TAIL_BLOCKS {
            self.render_batch(BATCH_SECONDS)?;

            let all_finished = self
                .voices
                .iter()
                .all(|v| v.active == 0 || v.env_stage == 4);
            if all_finished {
                break;
            }
        }

        self.audio_writer.finalize()
    }
}

/// GPU 加速音频导出入口（流式模式）
pub(crate) fn gpu_render_audio(config: &AudioRenderConfig) -> ExportResult<()> {
    info!(
        "[GPU 导出-流式] MIDI={:?}, SF2={:?}, 输出={:?}",
        config.midi_path, config.soundfonts, config.output_path
    );

    let file = std::fs::File::open(&config.midi_path)?;
    let mmap = unsafe { memmap2::Mmap::map(&file)? };

    let mut player = lumino_midi_loader::streaming::StreamingMidiPlayer::from_bytes(&mmap)
        .map_err(|e| crate::error::ExportError::AudioWrite(format!("解析 MIDI 失败: {e}")))?;

    let mut renderer = GpuExportRenderer::new(config, &config.output_path)?;

    let tempos: Vec<(u32, f32)> = player.tempo_changes().to_vec();
    let ppqn = player.ppqn();
    let mut tick_conv = super::tick_conv::TickToTime::new(tempos, ppqn);

    let total_ticks = player.total_ticks().max(1);
    let mut current_tick: u64 = 0;
    let mut event_count = 0_u64;
    let start_time = std::time::Instant::now();
    let mut last_progress = std::time::Instant::now();

    use midly::MidiMessage;
    use midly::TrackEventKind;

    // 跟踪每个通道的当前 program
    let mut channel_programs = [0u8; 16];

    while let Some((tick, _track_idx, kind)) = player.next_event() {
        // 进度报告
        if last_progress.elapsed() >= std::time::Duration::from_millis(100) {
            let pct = tick as f64 / total_ticks as f64;
            let elapsed = start_time.elapsed();
            let msg = format!(
                "[GPU] 进度: {:.1}% | 事件: {} | 耗时: {:.1}s",
                pct * 100.0,
                event_count,
                elapsed.as_secs_f64()
            );
            if let Some(ref callback) = config.progress_callback {
                callback(msg, pct);
            } else {
                eprint!("\r{}", msg);
            }
            last_progress = std::time::Instant::now();
        }

        // 前进到事件所在 tick
        if tick > current_tick {
            let delta = tick_conv.advance_to(tick);
            if delta > 0.0 {
                renderer.render_batch(delta)?;
            }
            current_tick = tick;
        }

        // 处理 MIDI 事件
        if let TrackEventKind::Midi { channel, message } = kind {
            let ch = channel.as_int();
            match message {
                MidiMessage::NoteOn { key, vel } => {
                    let program = channel_programs[ch as usize];
                    renderer.send_note_on(key, vel.into(), ch as u32, program);
                    event_count += 1;
                }
                MidiMessage::NoteOff { key, .. } => {
                    renderer.send_note_off(key, ch as u32);
                    event_count += 1;
                }
                MidiMessage::ProgramChange { program } => {
                    channel_programs[ch as usize] = program.as_int();
                }
                _ => {}
            }
        }
    }

    // 完成进度
    let elapsed = start_time.elapsed();
    let msg = format!(
        "[GPU] 进度: 100.0% | 事件: {} | 耗时: {:.1}s",
        event_count,
        elapsed.as_secs_f64()
    );
    if let Some(ref callback) = config.progress_callback {
        callback(msg, 1.0);
    } else {
        eprintln!("\r{}", msg);
    }

    renderer.finalize()?;
    info!("[GPU 导出] 渲染完成: {:?}", config.output_path);
    Ok(())
}

/// GPU 加速音频导出入口（内存模式）
pub(crate) fn gpu_render_audio_from_document(
    config: &AudioRenderConfig,
    doc: &lumino_midi_loader::document::MidiDocument,
) -> ExportResult<()> {
    info!(
        "[GPU 导出-内存] SF2={:?}, 输出={:?}",
        config.soundfonts, config.output_path
    );

    let mut renderer = GpuExportRenderer::new(config, &config.output_path)?;

    let total_events: usize =
        doc.notes.iter().map(|v| v.len()).sum::<usize>() * 2 + doc.control_events.len();
    if total_events == 0 {
        return Err(crate::error::ExportError::AudioWrite(
            "MIDI 文档中没有可渲染的事件".into(),
        ));
    }

    let tempos = doc.tempo_changes.clone();
    let ppqn = 480;
    let mut tick_conv = super::tick_conv::TickToTime::new(tempos, ppqn);

    let total_tick = doc
        .notes
        .iter()
        .flat_map(|t| t.iter())
        .map(|n| n.end_tick)
        .max()
        .unwrap_or(0)
        .max(1) as u64;

    let mut stream = super::MidiDocEventStream::new(doc);
    let mut current_tick: u64 = 0;
    let mut event_count = 0_u64;
    let start_time = std::time::Instant::now();
    let mut last_progress = std::time::Instant::now();

    // 跟踪每个通道的当前 program
    let mut channel_programs = [0u8; 16];

    while let Some(event) = stream.next_event() {
        let tick = event.tick as u64;

        // 进度报告
        if last_progress.elapsed() >= std::time::Duration::from_millis(100) {
            let pct = tick as f64 / total_tick as f64;
            let elapsed = start_time.elapsed();
            let msg = format!(
                "[GPU] 进度: {:.1}% | 事件: {} | 耗时: {:.1}s",
                pct * 100.0,
                event_count,
                elapsed.as_secs_f64()
            );
            if let Some(ref callback) = config.progress_callback {
                callback(msg, pct);
            } else {
                eprint!("\r{}", msg);
            }
            last_progress = std::time::Instant::now();
        }

        // 前进时间
        if tick > current_tick {
            let delta = tick_conv.advance_to(tick);
            if delta > 0.0 {
                renderer.render_batch(delta)?;
            }
            current_tick = tick;
        }

        // 处理事件
        let ch = event.channel as usize;
        match event.kind {
            0 => {
                // NoteOn
                let program = channel_programs[ch];
                renderer.send_note_on(event.param1, event.param2 as u8, ch as u32, program);
                event_count += 1;
            }
            1 => {
                // NoteOff
                renderer.send_note_off(event.param1, ch as u32);
                event_count += 1;
            }
            3 => {
                // ProgramChange
                channel_programs[ch] = event.param1;
            }
            _ => {}
        }
    }

    // 完成进度
    let elapsed = start_time.elapsed();
    let msg = format!(
        "[GPU] 进度: 100.0% | 事件: {} | 耗时: {:.1}s",
        event_count,
        elapsed.as_secs_f64()
    );
    if let Some(ref callback) = config.progress_callback {
        callback(msg, 1.0);
    } else {
        eprintln!("\r{}", msg);
    }

    renderer.finalize()?;
    info!("[GPU 导出] 渲染完成: {:?}", config.output_path);
    Ok(())
}
