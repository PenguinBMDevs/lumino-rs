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

/// 每批次渲染时长（秒）— 增大批次以减少 GPU 同步开销
const BATCH_SECONDS: f64 = 2.0;

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

/// 批量渲染路径中待处理的音符
#[derive(Clone, Copy)]
struct ScheduledNote {
    start_sample: u32,
    release_sample: u32,
    channel: u8,
    key: u8,
    velocity: u8,
    program: u8,
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

    /// 批量渲染路径的待处理音符队列
    pending_notes: Vec<ScheduledNote>,

    /// 进度回调（用于 render_full 分块过程中报告进度）
    progress_callback: Option<super::config::ProgressCallback>,

    sample_rate: u32,
    channel_count: u16,
    max_voices: u32,

    /// 是否已打印过 GPU 批次子阶段耗时（仅首次打印）
    gpu_batch_timing_printed: bool,
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
            pending_notes: Vec::new(),
            progress_callback: config.progress_callback.clone(),
            sample_rate,
            channel_count: channel_count as u16,
            max_voices: MAX_VOICES,
            gpu_batch_timing_printed: false,
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

    /// 渲染一个批次（按时长）
    fn render_batch(&mut self, duration: f64) -> ExportResult<()> {
        let batch_samples = (self.sample_rate as f64 * duration) as u32;
        self.render_batch_samples(batch_samples)
    }

    /// 渲染指定样点数
    ///
    /// 每次 dispatch 后回读 GPU 更新后的 voice 状态，确保跨批次时
    /// 包络、采样位置等状态正确延续。
    fn render_batch_samples(&mut self, batch_samples: u32) -> ExportResult<()> {
        if batch_samples == 0 {
            return Ok(());
        }

        self.gpu_renderer.ensure_output_capacity(batch_samples)?;

        if self.active_voices == 0 {
            let mut silent = vec![0.0f32; (batch_samples * self.channel_count as u32) as usize];
            self.audio_writer
                .write_samples(&mut silent)
                .map_err(|e| crate::error::ExportError::AudioWrite(format!("写入失败: {e}")))?;
            return Ok(());
        }

        // 压缩活跃 voice，记录原始索引以便回读后写回正确位置
        let t_cpu = std::time::Instant::now();
        let active_indices: Vec<u32> = self
            .voices
            .iter()
            .enumerate()
            .filter(|(_, v)| v.is_active != 0)
            .map(|(i, _)| i as u32)
            .collect();
        let active_voices: Vec<VoiceState> = active_indices
            .iter()
            .map(|&i| self.voices[i as usize])
            .collect();
        let active_count = active_voices.len() as u32;
        let t_collect = t_cpu.elapsed().as_secs_f64();

        let t_upload = std::time::Instant::now();
        self.gpu_renderer.upload_voice_states(&active_voices);
        let t_upload_el = t_upload.elapsed().as_secs_f64();

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
        let t_dispatch = std::time::Instant::now();
        self.gpu_renderer.dispatch(&params);
        let t_dispatch_el = t_dispatch.elapsed().as_secs_f64();

        let t_read = std::time::Instant::now();
        let mut output = self.gpu_renderer.read_output(batch_samples)?;
        let t_read_el = t_read.elapsed().as_secs_f64();

        // 回读 GPU 更新后的 voice 状态，按原始索引写回，并重建空闲栈
        let t_vs = std::time::Instant::now();
        let returned_states = self.gpu_renderer.read_voice_states(active_count)?;
        let t_vs_el = t_vs.elapsed().as_secs_f64();
        for (returned_idx, original_idx) in active_indices.iter().enumerate() {
            self.voices[*original_idx as usize] = returned_states[returned_idx];
        }
        self.rebuild_voice_metadata();

        if let Some(limiter) = &mut self.limiter {
            limiter.limit(&mut output);
        }

        let t_write = std::time::Instant::now();
        self.audio_writer
            .write_samples(&mut output)
            .map_err(|e| crate::error::ExportError::AudioWrite(format!("写入失败: {e}")))?;
        let t_write_el = t_write.elapsed().as_secs_f64();

        // 首次调用时打印子阶段耗时
        if !self.gpu_batch_timing_printed {
            self.gpu_batch_timing_printed = true;
            eprintln!(
                "[GPU 子阶段] collect={:.4}s upload={:.4}s dispatch={:.4}s read_output={:.4}s read_voices={:.4}s write={:.4}s  active_voices={}",
                t_collect, t_upload_el, t_dispatch_el, t_read_el, t_vs_el, t_write_el, active_count
            );
        }

        Ok(())
    }

    /// 根据 `voices` 数组重建 `active_voices` 与 `free_voices`
    fn rebuild_voice_metadata(&mut self) {
        self.active_voices = 0;
        self.free_voices.clear();
        for (idx, voice) in self.voices.iter().enumerate().rev() {
            if voice.is_active == 0 {
                self.free_voices.push(idx as u32);
            } else {
                self.active_voices += 1;
            }
        }
    }

    /// 按样点数前进 voice 状态，释放已结束的 voice
    fn advance_voices_samples(&mut self, samples: u32) {
        if samples == 0 {
            return;
        }
        for (idx, voice) in self.voices.iter_mut().enumerate() {
            if voice.is_active == 0 {
                continue;
            }
            voice.start_sample_offset = voice.start_sample_offset.saturating_sub(samples);
            if voice.release_sample_offset != 0xFFFFFFFF {
                if voice.release_sample_offset <= samples {
                    voice.env_time +=
                        (samples - voice.release_sample_offset) as f32 / self.sample_rate as f32;
                    if voice.env_time >= voice.envelope_release {
                        voice.is_active = 0;
                        self.active_voices -= 1;
                        self.free_voices.push(idx as u32);
                    }
                }
                voice.release_sample_offset = voice.release_sample_offset.saturating_sub(samples);
            }
        }
    }

    /// 查找活跃 voice 中最早的 release_sample_offset（不含无限长 voice）
    fn find_earliest_release(&self) -> Option<u32> {
        self.voices
            .iter()
            .filter(|v| v.is_active != 0 && v.release_sample_offset != 0xFFFFFFFF)
            .map(|v| v.release_sample_offset)
            .min()
    }

    /// 直接从 NoteEvent 加入渲染。
    ///
    /// 与 `note_on` 不同：此方法明确设置 `release_sample_offset`。
    /// 当 voice 池未满时立即创建 voice；否则加入待处理队列，由
    /// `render_full` 按时间窗口分块调度，从而支持超过 `MAX_VOICES` 的音符数。
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
        let note = ScheduledNote {
            start_sample: start_sample_offset,
            release_sample: release_sample_offset,
            channel: channel as u8,
            key,
            velocity,
            program,
        };
        if self.active_voices < self.max_voices {
            self.create_voice_from_scheduled_note(note, start_sample_offset, release_sample_offset);
        } else {
            self.pending_notes.push(note);
        }
    }

    /// 立即从 ScheduledNote 创建 voice（内部使用，调用方需保证 voice 池未满）
    fn create_voice_from_scheduled_note(
        &mut self,
        note: ScheduledNote,
        start_sample_offset: u32,
        release_sample_offset: u32,
    ) {
        let bank = 0usize;
        let program_idx = note.program as usize;

        let mut matched = false;
        let mut voice_setup = None;

        if let Some(b) = self.region_map.get(bank)
            && let Some(regions) = b.get(program_idx)
        {
            for entry in regions {
                if !entry.region.keyrange.contains(&note.key) {
                    continue;
                }
                if !entry.region.velrange.contains(&note.velocity) {
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
                let key_diff = (note.key as i16) - (region.root_key as i16);
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
                    key: note.key as u32,
                    channel: note.channel as u32,
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
                note.channel,
                note.key,
                note.velocity
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
        self.advance_voices_samples(samples);
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

    /// 渲染 `pending_notes` 中全部待处理音符，覆盖 `duration_seconds` 时长。
    ///
    /// 实现按时间窗口分块：
    /// - 每个窗口内只创建当前活跃的 voice，避免超过 `MAX_VOICES`；
    /// - 窗口之间回读 GPU voice 状态，保证跨批次包络与采样位置连续；
    /// - 若当前窗口内 voice 池临时满载，则进一步细分到最早 release 点。
    /// - 每 ~100ms 通过 `progress_callback` 报告进度。
    pub(crate) fn render_full(&mut self, duration_seconds: f64) -> ExportResult<()> {
        let total_samples = (self.sample_rate as f64 * duration_seconds) as u32;
        if total_samples == 0 {
            self.pending_notes.clear();
            return Ok(());
        }

        let batch_samples = (self.sample_rate as f64 * BATCH_SECONDS) as u32;

        // 分块窗口：使用大窗口以减少 GPU 同步次数
        // 窗口大小取总时长 / 20，上限 10 秒，下限 BATCH_SECONDS
        let window_seconds = (duration_seconds / 20.0).clamp(BATCH_SECONDS, 10.0);
        let window_samples = (self.sample_rate as f64 * window_seconds) as u32;
        let total_notes = self.active_voices as usize + self.pending_notes.len();
        let start_time = std::time::Instant::now();
        let mut last_progress_time = std::time::Instant::now();
        let mut rendered_events: u64 = 0;

        // 快速路径：所有音符能在单批次内完成
        if total_notes <= self.max_voices as usize && total_samples <= batch_samples {
            self.pending_notes.sort_by_key(|n| n.start_sample);
            let notes: Vec<ScheduledNote> = self.pending_notes.drain(..).collect();
            rendered_events = notes.len() as u64;
            for note in notes {
                self.create_voice_from_scheduled_note(note, note.start_sample, note.release_sample);
            }
            if self.active_voices == 0 {
                self.report_progress(1.0, rendered_events, start_time);
                return Ok(());
            }
            let result = self.render_batch_samples(total_samples);
            self.report_progress(1.0, rendered_events, start_time);
            return result;
        }

        // 分块路径
        self.pending_notes.sort_by_key(|n| n.start_sample);

        let mut cursor = 0usize;
        let mut current_sample = 0u32;

        while current_sample < total_samples || cursor < self.pending_notes.len() {
            let window_end = if current_sample < total_samples {
                (current_sample + window_samples).min(total_samples)
            } else {
                // 时间已耗尽，但还有待处理音符。
                // 扩展 window 来继续渲染子批次，以释放 voice 槽并创建新 voice。
                current_sample + window_samples
            };

            // 子批次循环：处理 voice 池临时满载
            loop {
                while cursor < self.pending_notes.len()
                    && self.pending_notes[cursor].start_sample < window_end
                    && self.active_voices < self.max_voices
                {
                    let note = self.pending_notes[cursor];
                    let rel_start = note.start_sample.saturating_sub(current_sample);
                    let rel_release = note.release_sample.saturating_sub(current_sample);
                    self.create_voice_from_scheduled_note(note, rel_start, rel_release);
                    rendered_events += 1;
                    cursor += 1;
                }

                if cursor < self.pending_notes.len()
                    && self.pending_notes[cursor].start_sample < window_end
                    && self.active_voices >= self.max_voices
                    && let Some(next_release) = self.find_earliest_release()
                {
                    let sub_end = next_release.min(window_end);
                    let sub_samples = sub_end.saturating_sub(current_sample);
                    if sub_samples > 0 {
                        self.render_batch_samples(sub_samples)?;
                        self.advance_voices_samples(sub_samples);
                        current_sample += sub_samples;
                        if last_progress_time.elapsed() >= std::time::Duration::from_millis(100) {
                            let pct = current_sample as f64 / total_samples as f64;
                            self.report_progress(pct.min(1.0), rendered_events, start_time);
                            last_progress_time = std::time::Instant::now();
                        }
                        continue;
                    }
                }
                break;
            }

            let remaining = window_end.saturating_sub(current_sample);
            if remaining > 0 {
                self.render_batch_samples(remaining)?;
                self.advance_voices_samples(remaining);
                current_sample = window_end;
                if last_progress_time.elapsed() >= std::time::Duration::from_millis(100) {
                    let pct = current_sample as f64 / total_samples as f64;
                    self.report_progress(pct.min(1.0), rendered_events, start_time);
                    last_progress_time = std::time::Instant::now();
                }
            }
        }

        self.pending_notes.clear();
        self.report_progress(1.0, rendered_events, start_time);
        Ok(())
    }

    /// 通过 `progress_callback` 报告当前进度
    fn report_progress(&self, pct: f64, event_count: u64, start_time: std::time::Instant) {
        let elapsed = start_time.elapsed();
        let msg = format!(
            "[GPU] 进度: {:.1}% | 事件: {} | 耗时: {:.1}s",
            pct * 100.0,
            event_count,
            elapsed.as_secs_f64()
        );
        if let Some(ref callback) = self.progress_callback {
            callback(msg, pct);
        } else {
            eprint!("\r{}", msg);
        }
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

            // 进度报告：每 100ms 一次
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

    // 将所有音符加入待渲染队列，由 render_full 按时间窗口分块处理
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

    if renderer.pending_notes.is_empty() && renderer.active_voices == 0 {
        return Err(crate::error::ExportError::AudioWrite(
            "没有成功创建任何 voice".into(),
        ));
    }

    renderer.render_full(total_seconds)?;

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

        // ===== 阶段 1: MIDI 加载 =====
        let t0 = Instant::now();
        let doc = MidiDocument::from_notes_file(&midi_path, None).expect("加载 MIDI 失败");
        let t_midi_load = t0.elapsed().as_secs_f64();
        eprintln!("[阶段 1] MIDI 加载: {:.3}s", t_midi_load);

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

        // ===== 阶段 2: GPU 渲染器创建（SF2 加载 + wgpu 初始化）=====
        let t1 = Instant::now();
        let mut renderer = super::GpuExportRenderer::new(&config, &config.output_path)
            .expect("创建 GPU 渲染器失败");
        let t_renderer_create = t1.elapsed().as_secs_f64();
        eprintln!("[阶段 2] GPU 渲染器创建 (SF2+wgpu): {:.3}s", t_renderer_create);

        // ===== 阶段 3: 时间转换与 program 表准备 =====
        let t2 = Instant::now();
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
        let programs = super::build_program_table(&doc.control_events);
        let t_tick_prep = t2.elapsed().as_secs_f64();
        eprintln!("[阶段 3] 时间转换+program 表: {:.3}s", t_tick_prep);
        eprintln!("[GPU 速度测试] 总时长: {:.2}s", total_seconds);

        // ===== 阶段 4: 音符调度（add_note 循环）=====
        let t3 = Instant::now();
        let mut note_count = 0u64;
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
                note_count += 1;
            }
        }
        let t_note_schedule = t3.elapsed().as_secs_f64();
        eprintln!(
            "[阶段 4] 音符调度 ({} notes): {:.3}s ({:.0} notes/s)",
            note_count,
            t_note_schedule,
            note_count as f64 / t_note_schedule
        );

        assert!(renderer.active_voices > 0, "没有成功创建任何 voice");
        eprintln!("[GPU 速度测试] 活跃 voice 数: {}", renderer.active_voices);

        // ===== 阶段 5: GPU 渲染 =====
        let t4 = Instant::now();
        renderer.render_full(total_seconds).expect("渲染失败");
        let t_render = t4.elapsed().as_secs_f64();

        let speed = rendered_notes as f64 / t_render;
        eprintln!("[阶段 5] GPU 渲染耗时: {:.3}s", t_render);

        // ===== 阶段 6: finalize =====
        let t5 = Instant::now();
        renderer.finalize().expect("finalize 失败");
        let t_finalize = t5.elapsed().as_secs_f64();
        eprintln!("[阶段 6] finalize: {:.3}s", t_finalize);

        // ===== 汇总 =====
        let total_time = t_midi_load + t_renderer_create + t_tick_prep + t_note_schedule + t_render + t_finalize;
        eprintln!("═══════════════════════════════════════════════════════════════");
        eprintln!("[汇总] 各阶段耗时占比:");
        eprintln!(
            "  阶段 1 MIDI 加载:       {:>8.3}s  ({:5.1}%)",
            t_midi_load,
            t_midi_load / total_time * 100.0
        );
        eprintln!(
            "  阶段 2 GPU 渲染器创建:  {:>8.3}s  ({:5.1}%)",
            t_renderer_create,
            t_renderer_create / total_time * 100.0
        );
        eprintln!(
            "  阶段 3 时间转换+program: {:>8.3}s  ({:5.1}%)",
            t_tick_prep,
            t_tick_prep / total_time * 100.0
        );
        eprintln!(
            "  阶段 4 音符调度:        {:>8.3}s  ({:5.1}%)",
            t_note_schedule,
            t_note_schedule / total_time * 100.0
        );
        eprintln!(
            "  阶段 5 GPU 渲染:        {:>8.3}s  ({:5.1}%)",
            t_render,
            t_render / total_time * 100.0
        );
        eprintln!(
            "  阶段 6 finalize:        {:>8.3}s  ({:5.1}%)",
            t_finalize,
            t_finalize / total_time * 100.0
        );
        eprintln!(
            "  总计:                   {:>8.3}s",
            total_time
        );
        eprintln!("═══════════════════════════════════════════════════════════════");
        eprintln!("渲染音符数: {}", rendered_notes);
        eprintln!("平均速度: {:.2} 音符/s", speed);

        assert!(speed > 500_000.0, "渲染速度未达标：{:.2} < 500000", speed);
    }
}
