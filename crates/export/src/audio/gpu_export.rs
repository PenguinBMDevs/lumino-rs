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
const MAX_VOICES: u32 = 65536;

/// 每批次渲染时长（秒）
const BATCH_SECONDS: f64 = 0.5;

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

    /// 空闲 voice 索引栈，避免 find_free_voice 扫描整个数组
    free_voices: Vec<u32>,

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
            free_voices: (0..MAX_VOICES).rev().collect(),
            sample_rate,
            channel_count: channel_count as u16,
            max_voices: MAX_VOICES,
        })
    }

    /// 从 SF2 预设中提取样本数据，构建扁平化缓冲区 + 区域查找表
    ///
    /// # 样本去重
    ///
    /// SF2 音色库中多个 region 可能共享同一份样本数据（通过 `Arc<[f32]>`
    /// 指针共享）。注意：`xsynth_soundfonts` 的 `build_region_samples()` 为
    /// 每个 region 创建独立的 `Arc<[Arc<[f32]>]>`（外层），但内层 `Arc<[f32]>`
    /// 是共享的。因此使用内层 Arc 指针 `(channel0_ptr, channel1_ptr)` 作为
    /// HashMap key 检测重复，只复制每个唯一样本一次，避免 OOM。
    fn extract_samples(presets: &[Sf2Preset]) -> ExportResult<(Vec<f32>, RegionMap)> {
        let mut all_samples: Vec<f32> = Vec::new();
        let mut bank_map: RegionMap = Vec::new();
        // 样本去重映射：内层 Arc<[f32]> 指针 → 扁平缓冲区中的偏移
        // 键为 (channel0_ptr as usize, channel1_ptr as usize)
        // 单声道时 channel1_ptr = 0
        let mut sample_map: HashMap<(usize, usize), u32> = HashMap::new();

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
                let sample_arcs = region.sample.as_ref();
                let dedup_key = if sample_arcs.len() >= 2 {
                    (
                        Arc::as_ptr(&sample_arcs[0]) as *const f32 as usize,
                        Arc::as_ptr(&sample_arcs[1]) as *const f32 as usize,
                    )
                } else if sample_arcs.len() == 1 {
                    (Arc::as_ptr(&sample_arcs[0]) as *const f32 as usize, 0)
                } else {
                    (0, 0)
                };

                let sample_offset = if let Some(&offset) = sample_map.get(&dedup_key) {
                    // 样本已存在，复用偏移
                    offset
                } else {
                    let offset = all_samples.len() as u32;

                    // 展开样本数据（立体声交错）
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

                    sample_map.insert(dedup_key, offset);
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
    pub(crate) fn note_on(
        &mut self,
        channel: u32,
        key: u8,
        velocity: u8,
        program: u8,
        sample_offset: u32,
    ) {
        if velocity == 0 {
            self.note_off(channel, key, sample_offset);
            return;
        }
        if self.active_voices >= self.max_voices {
            tracing::debug!("[GPU 导出] voice 池已满，跳过 NoteOn");
            return;
        }

        // 查找匹配的区域
        let bank = 0usize; // 默认 bank 0
        let program_idx = program as usize;

        let mut matched = false;
        let mut voice_setup = None;

        if let Some(b) = self.region_map.get(bank)
            && let Some(regions) = b.get(program_idx)
        {
            for entry in regions {
                if !entry.region.keyrange.contains(&key) {
                    continue;
                }
                if !entry.region.velrange.contains(&velocity) {
                    continue;
                }

                // 找到匹配区域，记录参数，脱离 borrow
                voice_setup = Some((entry.region.clone(), entry.sample_offset));
                matched = true;
                break; // 只匹配第一个区域
            }
        }

        if let Some((region, sample_offset_idx)) = voice_setup {
            // 找一个空闲 voice 槽
            let voice_idx = self.find_free_voice();
            if voice_idx < self.max_voices {
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

                let new_voice = VoiceState {
                    sample_pos: region.offset as f32,
                    pitch_ratio,
                    volume: volume_linear,
                    pan_left,
                    pan_right,
                    loop_start: region.loop_start as f32,
                    loop_end: region.loop_end as f32,
                    loop_mode: match region.loop_mode {
                        xsynth_soundfonts::LoopMode::LoopContinuous => 1,
                        _ => 0,
                    },
                    sample_index: sample_offset_idx,
                    envelope_attack: env.ampeg_attack,
                    envelope_decay: env.ampeg_decay,
                    envelope_sustain: env.ampeg_sustain,
                    envelope_release: env.ampeg_release,
                    envelope_value: 0.0,
                    env_stage: 0, // attack
                    env_time: 0.0,
                    is_active: 1,
                    start_sample_offset: sample_offset,
                    release_sample_offset: 0xFFFFFFFF,
                    key: key as u32,
                    channel,
                    _pad0: 0,
                    _pad1: 0,
                    _pad2: 0,
                };

                self.voices[voice_idx as usize] = new_voice;
                self.active_voices += 1;
            }
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
    pub(crate) fn note_off(&mut self, channel: u32, key: u8, sample_offset: u32) {
        for voice in self.voices.iter_mut() {
            if voice.is_active == 0 {
                continue;
            }
            if voice.key == key as u32
                && voice.channel == channel
                && voice.release_sample_offset == 0xFFFFFFFF
            {
                voice.release_sample_offset = sample_offset;
            }
        }
    }

    /// 查找空闲 voice 槽
    ///
    /// 使用 `free_voices` 栈实现 O(1) 分配，避免 voice 池接近满时扫描整个数组。
    fn find_free_voice(&mut self) -> u32 {
        self.free_voices.pop().unwrap_or(self.max_voices)
    }

    /// 渲染一个批次
    fn render_batch(&mut self, duration: f64) -> ExportResult<()> {
        let batch_samples = (self.sample_rate as f64 * duration) as u32;
        if batch_samples == 0 {
            return Ok(());
        }

        if self.active_voices == 0 {
            // 没有活跃 voice，输出静音
            let silent = vec![0.0f32; (batch_samples * self.channel_count as u32) as usize];
            self.audio_writer
                .write_samples(&mut silent.clone())
                .map_err(|e| crate::error::ExportError::AudioWrite(format!("写入失败: {e}")))?;
            return Ok(());
        }

        // 压缩活跃 voice，避免上传完整的 max_voices 状态
        let active_voices: Vec<VoiceState> = self
            .voices
            .iter()
            .filter(|v| v.is_active != 0)
            .copied()
            .collect();
        let active_count = active_voices.len() as u32;

        // 上传活跃 voice 状态
        self.gpu_renderer.upload_voice_states(&active_voices);

        // 调度 GPU 计算
        let params = RenderParams {
            sample_rate: self.sample_rate as f32,
            num_voices: active_count,
            num_samples: batch_samples,
            output_offset: 0,
            max_voices: active_count,
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
        };
        self.gpu_renderer.dispatch(&params);

        // 读取输出
        let mut output = self.gpu_renderer.read_output(batch_samples)?;

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

    /// 直接从 NoteEvent 创建一个 voice，用于一次性批量渲染路径。
    ///
    /// 与 `note_on` 不同：此方法明确设置 `release_sample_offset`，
    /// 使 GPU 能在单个大 batch 内处理完整的音符生命周期。
    pub(crate) fn add_note(
        &mut self,
        channel: u32,
        key: u8,
        velocity: u8,
        program: u8,
        start_sample_offset: u32,
        release_sample_offset: u32,
    ) {
        if velocity == 0 {
            return;
        }
        if self.active_voices >= self.max_voices {
            return;
        }

        let bank = 0usize;
        let program_idx = program as usize;

        let mut matched = false;
        let mut voice_setup = None;

        if let Some(b) = self.region_map.get(bank)
            && let Some(regions) = b.get(program_idx)
        {
            for entry in regions {
                if !entry.region.keyrange.contains(&key) {
                    continue;
                }
                if !entry.region.velrange.contains(&velocity) {
                    continue;
                }
                voice_setup = Some((entry.region.clone(), entry.sample_offset));
                matched = true;
                break;
            }
        }

        if let Some((region, sample_offset_idx)) = voice_setup {
            let voice_idx = self.find_free_voice();
            if voice_idx < self.max_voices {
                let sample_rate_ratio = region.sample_rate as f32 / self.sample_rate as f32;
                let key_diff = (key as i16) - (region.root_key as i16);
                let pitch_ratio = sample_rate_ratio * 2.0f32.powf(key_diff as f32 / 12.0);
                let pan = (region.pan as f32 + 50.0) / 100.0;
                let pan_left = (1.0 - pan).sqrt();
                let pan_right = pan.sqrt();
                let env = &region.ampeg_envelope;
                let volume_linear = 10.0f32.powf(region.volume / 20.0);

                self.voices[voice_idx as usize] = VoiceState {
                    sample_pos: region.offset as f32,
                    pitch_ratio,
                    volume: volume_linear,
                    pan_left,
                    pan_right,
                    loop_start: region.loop_start as f32,
                    loop_end: region.loop_end as f32,
                    loop_mode: match region.loop_mode {
                        xsynth_soundfonts::LoopMode::LoopContinuous => 1,
                        _ => 0,
                    },
                    sample_index: sample_offset_idx,
                    envelope_attack: env.ampeg_attack,
                    envelope_decay: env.ampeg_decay,
                    envelope_sustain: env.ampeg_sustain,
                    envelope_release: env.ampeg_release,
                    envelope_value: 0.0,
                    env_stage: 0,
                    env_time: 0.0,
                    is_active: 1,
                    start_sample_offset,
                    release_sample_offset,
                    key: key as u32,
                    channel,
                    _pad0: 0,
                    _pad1: 0,
                    _pad2: 0,
                };
                self.active_voices += 1;
            }
        }

        if !matched {
            tracing::debug!(
                "[GPU 导出] add_note(ch={}, key={}, vel={}): 未找到匹配区域",
                channel,
                key,
                velocity
            );
        }
    }

    /// 发送 MIDI 事件
    pub(crate) fn send_note_on(
        &mut self,
        key: u8,
        velocity: u8,
        channel: u32,
        program: u8,
        sample_offset: u32,
    ) {
        self.note_on(channel, key, velocity, program, sample_offset);
    }

    pub(crate) fn send_note_off(&mut self, key: u8, channel: u32, sample_offset: u32) {
        self.note_off(channel, key, sample_offset);
    }

    pub(crate) fn advance_voices(&mut self, duration: f64) {
        let samples = (self.sample_rate as f64 * duration) as u32;
        for (idx, voice) in self.voices.iter_mut().enumerate() {
            if voice.is_active != 0 {
                voice.start_sample_offset = voice.start_sample_offset.saturating_sub(samples);
                if voice.release_sample_offset != 0xFFFFFFFF {
                    if voice.release_sample_offset <= samples {
                        // 开始 release
                        voice.env_time += (samples - voice.release_sample_offset) as f32
                            / self.sample_rate as f32;
                        if voice.env_time >= voice.envelope_release {
                            voice.is_active = 0;
                            self.active_voices -= 1;
                            self.free_voices.push(idx as u32);
                        }
                    }
                    voice.release_sample_offset =
                        voice.release_sample_offset.saturating_sub(samples);
                }
            }
        }
    }

    /// 完成渲染
    pub(crate) fn finalize(mut self) -> ExportResult<()> {
        // 渲染尾部（所有 voice 进入 release 直至静音）
        for _ in 0..TAIL_BLOCKS {
            self.render_batch(BATCH_SECONDS)?;

            let all_finished = self
                .voices
                .iter()
                .all(|v| v.is_active == 0 || v.env_stage == 4);
            if all_finished {
                break;
            }
        }

        self.audio_writer.finalize()
    }

    /// 一次性渲染指定时长的全部活跃 voice
    ///
    /// 用于 `gpu_render_audio_from_document` 优化路径：当总音符数不超过
    /// `MAX_VOICES` 时，可把整个文档作为单个 batch 渲染，避免频繁的
    /// CPU-GPU 同步。
    pub(crate) fn render_full(&mut self, duration_seconds: f64) -> ExportResult<()> {
        let total_samples = (self.sample_rate as f64 * duration_seconds) as u32;
        if total_samples == 0 || self.active_voices == 0 {
            return Ok(());
        }

        // 确保输出缓冲区足够大
        self.gpu_renderer.ensure_output_capacity(total_samples)?;

        // 压缩活跃 voice
        let active_voices: Vec<VoiceState> = self
            .voices
            .iter()
            .filter(|v| v.is_active != 0)
            .copied()
            .collect();
        let active_count = active_voices.len() as u32;

        self.gpu_renderer.upload_voice_states(&active_voices);

        let params = RenderParams {
            sample_rate: self.sample_rate as f32,
            num_voices: active_count,
            num_samples: total_samples,
            output_offset: 0,
            max_voices: active_count,
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
        };
        self.gpu_renderer.dispatch(&params);

        let mut output = self.gpu_renderer.read_output(total_samples)?;

        if let Some(limiter) = &mut self.limiter {
            limiter.limit(&mut output);
        }

        self.audio_writer
            .write_samples(&mut output)
            .map_err(|e| crate::error::ExportError::AudioWrite(format!("写入失败: {e}")))?;

        Ok(())
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

    let mut current_time = 0.0;
    let mut batch_end_time = BATCH_SECONDS;

    while let Some((tick, _track_idx, kind)) = player.next_event() {
        // 前进时间
        if tick > current_tick {
            let delta = tick_conv.advance_to(tick);
            if delta > 0.0 {
                current_time += delta;
            }
            current_tick = tick;
        }

        // 处理跨越批次边界的渲染
        while current_time >= batch_end_time {
            renderer.render_batch(BATCH_SECONDS)?;
            renderer.advance_voices(BATCH_SECONDS);
            batch_end_time += BATCH_SECONDS;

            // 进度报告
            if last_progress.elapsed() >= std::time::Duration::from_millis(500) {
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
        }

        let sample_offset =
            ((current_time - (batch_end_time - BATCH_SECONDS)) * config.sample_rate as f64) as u32;

        // 处理 MIDI 事件
        if let TrackEventKind::Midi { channel, message } = kind {
            let ch = channel.as_int();
            match message {
                MidiMessage::NoteOn { key, vel } => {
                    let program = channel_programs[ch as usize];
                    renderer.send_note_on(key, vel.into(), ch as u32, program, sample_offset);
                    event_count += 1;
                }
                MidiMessage::NoteOff { key, .. } => {
                    renderer.send_note_off(key, ch as u32, sample_offset);
                    event_count += 1;
                }
                MidiMessage::ProgramChange { program } => {
                    channel_programs[ch as usize] = program.as_int();
                }
                _ => {}
            }
        }
    }

    // 渲染最后一个不完整的批次
    let remainder = current_time - (batch_end_time - BATCH_SECONDS);
    if remainder > 0.0 {
        renderer.render_batch(remainder)?;
        renderer.advance_voices(remainder);
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

/// 构建每个通道的 ProgramChange 查找表，按 tick 升序排列。
fn build_program_table(
    control_events: &[midly::loader::PackedControlEvent],
) -> [Vec<(u32, u8)>; 16] {
    let mut table: [Vec<(u32, u8)>; 16] = std::array::from_fn(|_| Vec::new());
    for ctrl in control_events {
        if ctrl.kind == 1 {
            // kind == 1 表示 ProgramChange
            table[ctrl.channel as usize].push((ctrl.tick, ctrl.as_program_change()));
        }
    }
    table
}

/// 查询指定 tick 处通道的 program，默认返回 0。
fn program_at(programs: &[Vec<(u32, u8)>; 16], channel: u8, tick: u32) -> u8 {
    let list = &programs[channel as usize];
    let idx = list.partition_point(|(t, _)| *t <= tick);
    if idx == 0 { 0 } else { list[idx - 1].1 }
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

    let total_notes: usize = doc.notes.iter().map(|v| v.len()).sum();
    if total_notes == 0 {
        return Err(crate::error::ExportError::AudioWrite(
            "MIDI 文档中没有可渲染的音符".into(),
        ));
    }

    let tempos = doc.tempo_changes.clone();
    let ppqn = 480;
    let tick_conv = super::tick_conv::TickToTime::new(tempos, ppqn);

    // 获取总渲染时长（秒）
    let total_tick = doc
        .notes
        .iter()
        .flat_map(|t| t.iter())
        .map(|n| n.end_tick)
        .max()
        .unwrap_or(0)
        .max(1);
    let total_seconds = tick_conv.tick_to_seconds(total_tick as u64);

    let programs = build_program_table(&doc.control_events);

    // 一次性创建所有 voice
    for track in &doc.notes {
        for note in track {
            let program = program_at(&programs, note.channel, note.start_tick);
            let start_time = tick_conv.tick_to_seconds(note.start_tick as u64);
            let release_time = tick_conv.tick_to_seconds(note.end_tick as u64);
            let start_sample = (start_time * config.sample_rate as f64) as u32;
            let release_sample = (release_time * config.sample_rate as f64) as u32;

            renderer.add_note(
                note.channel as u32,
                note.key,
                note.velocity,
                program,
                start_sample,
                release_sample,
            );
        }
    }

    if renderer.active_voices == 0 {
        return Err(crate::error::ExportError::AudioWrite(
            "没有成功创建任何 voice".into(),
        ));
    }

    // 进度回调
    if let Some(ref callback) = config.progress_callback {
        callback("[GPU] 进度: 0.0% | 事件: 0 | 耗时: 0.0s".to_string(), 0.0);
    }

    let render_start = std::time::Instant::now();
    renderer.render_full(total_seconds)?;
    let render_elapsed = render_start.elapsed().as_secs_f64();
    eprintln!("[GPU 导出] render_full 耗时: {:.3}s", render_elapsed);

    // 完成进度
    if let Some(ref callback) = config.progress_callback {
        callback("[GPU] 进度: 100.0% | 事件: 0 | 耗时: 0.0s".to_string(), 1.0);
    } else {
        eprintln!("\r[GPU] 进度: 100.0% | 事件: 0 | 耗时: 0.0s");
    }

    renderer.finalize()?;
    info!("[GPU 导出] 渲染完成: {:?}", config.output_path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::config::{AudioChannelMode, AudioInterpolation, ThreadMode};
    use lumino_midi_loader::document::MidiDocument;
    use std::path::PathBuf;
    use std::time::Instant;

    #[test]
    fn test_gpu_audio_export_speed() {
        let midi_path = PathBuf::from("../../test-file/test_note_worker_bench_assets/Erosoul.mid");
        let sf2_path = PathBuf::from("../../test-file/test.sf2");
        let output_path = std::env::temp_dir().join("bench_gpu_render.wav");

        assert!(midi_path.exists(), "MIDI 文件不存在: {:?}", midi_path);
        assert!(sf2_path.exists(), "SF2 文件不存在: {:?}", sf2_path);

        let doc = MidiDocument::from_notes_file(&midi_path, None).expect("加载 MIDI 失败");

        let config = AudioRenderConfig {
            midi_path: midi_path.clone(),
            soundfonts: vec![sf2_path],
            output_path: output_path.clone(),
            sample_rate: 44100,
            channels: AudioChannelMode::Stereo,
            layer_limit: None,
            channel_threading: ThreadMode::Auto,
            key_threading: ThreadMode::Auto,
            interpolation: AudioInterpolation::Linear,
            apply_limiter: false,
            disable_fade_out: false,
            linear_envelope: false,
            use_gpu: true,
            progress_callback: None,
        };

        // 统计完整 MIDI 的音符数
        let rendered_notes: usize = doc.notes.iter().map(|v| v.len()).sum();

        eprintln!("[GPU 速度测试] 总音符数: {}", rendered_notes);

        // 创建渲染器（包含 SF2 加载与 wgpu 初始化，不计入 GPU 渲染时间）
        let mut renderer = super::GpuExportRenderer::new(&config, &config.output_path)
            .expect("创建 GPU 渲染器失败");

        // 准备时间转换与 program 表
        let tempos = doc.tempo_changes.clone();
        let ppqn = 480;
        let tick_conv = super::super::tick_conv::TickToTime::new(tempos, ppqn);
        let total_tick = doc
            .notes
            .iter()
            .flat_map(|t| t.iter())
            .map(|n| n.end_tick)
            .max()
            .unwrap_or(0)
            .max(1);
        let total_seconds = tick_conv.tick_to_seconds(total_tick as u64);
        eprintln!("[GPU 速度测试] 总时长: {:.2}s", total_seconds);

        // 一次性创建所有 voice
        let programs = super::build_program_table(&doc.control_events);
        for track in &doc.notes {
            for note in track {
                let program = super::program_at(&programs, note.channel, note.start_tick);
                let start_time = tick_conv.tick_to_seconds(note.start_tick as u64);
                let release_time = tick_conv.tick_to_seconds(note.end_tick as u64);
                let start_sample = (start_time * config.sample_rate as f64) as u32;
                let release_sample = (release_time * config.sample_rate as f64) as u32;

                renderer.add_note(
                    note.channel as u32,
                    note.key,
                    note.velocity,
                    program,
                    start_sample,
                    release_sample,
                );
            }
        }

        assert!(renderer.active_voices > 0, "没有成功创建任何 voice");
        eprintln!("[GPU 速度测试] 活跃 voice 数: {}", renderer.active_voices);

        // 仅统计实际 GPU 渲染时间（不含初始化、SF2 加载、voice 准备）
        let render_start = Instant::now();
        renderer.render_full(total_seconds).expect("渲染失败");
        let render_elapsed = render_start.elapsed().as_secs_f64();

        let speed = rendered_notes as f64 / render_elapsed;
        println!("GPU 实际渲染耗时: {:.3}s", render_elapsed);
        println!("渲染音符数: {}", rendered_notes);
        println!("平均速度: {:.2} 音符/s", speed);

        assert!(speed > 500_000.0, "渲染速度未达标：{:.2} < 500000", speed);

        renderer.finalize().expect("finalize 失败");
    }
}
