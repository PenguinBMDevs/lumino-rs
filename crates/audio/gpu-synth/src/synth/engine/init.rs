use super::*;

use crate::gpu::ChunkParams;

impl GpuSynth {
    /// Creates a new synthesizer with the given configuration, initializing
    /// the GPU device (a wgpu adapter is picked automatically).
    ///
    /// # Errors
    ///
    /// Returns [`SynthError::GpuInit`] if no GPU device can be created.
    pub fn new(config: SynthConfig) -> Result<Self, SynthError> {
        config.validate()?;
        let ctx = Arc::new(create_gpu_context()?);
        let res = GpuResources::new(ctx, config.block_size, config.max_voices)?;
        Self::with_resources(config, res)
    }

    /// 离线导出专用构造（REND-019 #152）。
    ///
    /// 与 [`Self::new`] 的差异仅在 device-lost 语义：离线上下文的丢失**只置
    /// 本地标志、不转发进程级钩子**——导出的独立 device 丢失不应误触发实时
    /// LGS 重建链路；导出侧经 [`Self::is_device_lost`] / 有界读回等待快速
    /// 以明确错误收场（可重试）。
    ///
    /// # Errors
    ///
    /// 同 [`Self::new`]。
    pub fn new_offline(config: SynthConfig) -> Result<Self, SynthError> {
        config.validate()?;
        let ctx = Arc::new(crate::gpu::create_gpu_context_offline()?);
        let res = GpuResources::new(ctx, config.block_size, config.max_voices)?;
        Self::with_resources(config, res)
    }

    /// Creates a synthesizer reusing an existing [`GpuResources`] (advanced
    /// use: multiple engines sharing one device).
    ///
    /// # Errors
    ///
    /// Returns [`SynthError::Config`] if the configuration is invalid.
    pub fn with_resources(config: SynthConfig, res: GpuResources) -> Result<Self, SynthError> {
        config.validate()?;
        let device = &res.ctx.device;
        let queue = &res.ctx.queue;
        let block = config.block_size;
        let max_voices = config.max_voices;
        let midi_channels = config.midi_channels;
        // Physical pool: active voices + room for one block's worth of
        // fading (trimmed) voices. See `FADE_SLOTS_FRACTION`.
        // When max_voices == 0 (unlimited / black-MIDI mode) the pool is
        // just an initial hint - buffers grow on demand without trimming.
        let pool = if max_voices == 0 {
            4096usize
        } else {
            max_voices + max_voices / FADE_SLOTS_FRACTION
        };

        let params_buf = GrowableBuffer::new(
            device,
            queue,
            "voice params dyn",
            (VoiceParamsDyn::SIZE * pool) as u64,
            wgpu::BufferUsages::STORAGE,
        );
        let params_static_buf = GrowableBuffer::new(
            device,
            queue,
            "voice params static",
            (VoiceParamsStatic::SIZE * pool) as u64,
            wgpu::BufferUsages::STORAGE,
        );
        let samples_chunks = (0..SAMPLES_CHUNKS)
            .map(|i| {
                GrowableBuffer::with_max_capacity(
                    device,
                    queue,
                    &format!("samples chunk {i}"),
                    1 << 20,
                    SAMPLES_CHUNK_BYTES,
                    wgpu::BufferUsages::STORAGE,
                )
            })
            .collect::<Vec<_>>();
        let sinc = crate::synth::dsp::build_sinc_table();
        let sinc_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sinc table"),
            size: (sinc.len() * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        res.ctx
            .queue
            .write_buffer(&sinc_buf, 0, bytemuck::cast_slice(&sinc));

        let env_buf = GrowableBuffer::new(
            device,
            queue,
            "env stages",
            (EnvStageGpu::SIZE * pool * 8) as u64,
            wgpu::BufferUsages::STORAGE,
        );
        let states_buf = GrowableBuffer::new(
            device,
            queue,
            "voice states",
            (VoiceState::SIZE * pool) as u64,
            wgpu::BufferUsages::STORAGE,
        );
        let chunk_size = res.chunk_size;
        let voice_out_buf = GrowableBuffer::with_max_capacity(
            device,
            queue,
            "voice out",
            (pool * chunk_size * 2 * 4) as u64,
            MAX_VOICE_OUT_BYTES,
            wgpu::BufferUsages::STORAGE,
        );
        let out_storage_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("out storage"),
            size: (block * 2 * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let out_readback = [
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("out readback 0"),
                size: (block * 2 * 4) as u64,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("out readback 1"),
                size: (block * 2 * 4) as u64,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        ];
        // Zero the output storage and readback buffers: wgpu does not zero
        // them, and any window where a readback is consumed before its copy
        // (e.g. the first block of a session) would feed uninitialized
        // garbage to the audio output (measured: recurring single-sample
        // pops of ~40000).
        {
            let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            enc.clear_buffer(&out_storage_buf, 0, Some((block * 2 * 4) as u64));
            enc.clear_buffer(&out_readback[0], 0, Some((block * 2 * 4) as u64));
            enc.clear_buffer(&out_readback[1], 0, Some((block * 2 * 4) as u64));
            queue.submit(Some(enc.finish()));
        }
        let states_readback = [
            GrowableBuffer::new(
                device,
                queue,
                "states readback 0",
                (VoiceState::SIZE * pool) as u64,
                wgpu::BufferUsages::MAP_READ,
            ),
            GrowableBuffer::new(
                device,
                queue,
                "states readback 1",
                (VoiceState::SIZE * pool) as u64,
                wgpu::BufferUsages::MAP_READ,
            ),
        ];
        let mix_buckets_buf = GrowableBuffer::new(
            device,
            queue,
            "mix buckets",
            ((MIX_BUCKET_OFFSETS + pool) * 4) as u64,
            wgpu::BufferUsages::STORAGE,
        );
        // Every (frame, channel) partial is written by the gather pass before
        // the reduce pass reads it, so an uninitialized allocation is safe.
        let mix_partial_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mix partial"),
            size: (chunk_size * config.midi_channels * 6 * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let chunks = config.block_size.div_ceil(chunk_size).max(1);
        let chunk_align = u64::from(
            device
                .limits()
                .min_uniform_buffer_offset_alignment
                .max(ChunkParams::SIZE as u32),
        );
        let chunk_params_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("chunk params"),
            size: (chunks as u64 * chunk_align).max(ChunkParams::SIZE as u64),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mix_events_buf = GrowableBuffer::new(
            device,
            queue,
            "mix events",
            16 << 10,
            wgpu::BufferUsages::STORAGE,
        );
        let mix_params_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mix params"),
            size: MixParams::SIZE as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Zero the dynamic storage buffers that are read every dispatch.
        let zero = vec![0u8; VoiceParamsDyn::SIZE * pool];
        if !zero.is_empty() {
            res.ctx.queue.write_buffer(params_buf.buffer(), 0, &zero);
        }
        let zero = vec![0u8; VoiceState::SIZE * pool];
        if !zero.is_empty() {
            res.ctx.queue.write_buffer(states_buf.buffer(), 0, &zero);
        }
        let zero = vec![0u8; EnvStageGpu::SIZE * pool * 8];
        if !zero.is_empty() {
            res.ctx.queue.write_buffer(env_buf.buffer(), 0, &zero);
        }

        // REND-015 #115: 每块 `apply_events` 开头会重置；这里只是给字段一个
        // 合法初值（首块应用事件前不会有裁剪）。
        let trim_fade_budget = trim_fade_budget_for(&config);

        let mut engine = Self {
            config,
            base_max_voices: max_voices,
            res,
            sf: None,
            params_dyn_buf: params_buf,
            params_static_buf,
            samples_chunks,
            sinc_buf,
            env_buf,
            states_buf,
            voice_out_buf,
            out_storage_buf,
            out_readback,
            out_readback_cur: 0,
            states_readback,
            states_readback_cur: 0,
            mix_buckets_buf,
            mix_partial_buf,
            chunk_params_buf,
            chunk_align,
            chunk_params_bytes: Vec::new(),
            mix_events_buf,
            mix_params_buf,
            render_bg: None,
            mix_bg: None,
            render_bg_dirty: true,
            mix_bg_dirty: true,
            channels: (0..midi_channels).map(|_| ChannelState::new()).collect(),
            voices: Vec::new(),
            key_voices: vec![VecDeque::new(); midi_channels * 128],
            sample_offsets: std::collections::HashMap::new(),
            samples_next_offset: 0,
            global_frame: 0,
            pending_events: VecDeque::new(),
            offline_events: Vec::new(),
            offline_cursor: 0,
            pending_mix_events: Vec::new(),
            active_voice_count: 0,
            peak_voice_count: 0,
            voice_frames: 0,
            limiter_gain: 1.0,
            limiter_tail: Vec::new(),
            last_out: None,
            last_states: None,
            prev_voice_ids: Vec::new(),
            scratch_voice_ids: Vec::new(),
            mix_in_block: Vec::new(),
            mix_events: Vec::new(),
            mix_starts: Vec::new(),
            upload_params: Vec::new(),
            upload_states: Vec::new(),
            upload_chans: Vec::new(),
            static_table: Vec::new(),
            slot_free: Vec::new(),
            slot_count: 0,
            dirty_slots: Vec::new(),
            static_runs: Vec::new(),
            env_pool: EnvTemplatePool::default(),
            fade_env_template: u32::MAX,
            mix_bucket_offsets: Vec::new(),
            mix_bucket_cursor: Vec::new(),
            mix_bucket_indices: Vec::new(),
            note_counter: 0,
            voice_id_counter: 0,
            spawn_budget: vec![0; midi_channels * 128],
            trim_fade_budget,
            active_notes: vec![0; midi_channels * 128],
            voice_templates: std::collections::HashMap::new(),
            states_sync_counter: 0,
            pending: None,
            belt: wgpu::util::StagingBelt::new(1 << 20),
            render_checkpoint: None,
            render_progress: None,
        };
        engine.rebuild_bind_groups();
        Ok(engine)
    }

    /// Returns the engine configuration.
    pub fn config(&self) -> &SynthConfig {
        &self.config
    }

    /// 当前渲染帧位置（音频时间线；REND-016 #139 过载重同步判定用）。
    pub fn global_frame(&self) -> u64 {
        self.global_frame
    }

    /// Installs (or clears) the cooperative checkpoint used by the offline
    /// render loops for cancellation/pause.
    ///
    /// The closure runs once per rendered block (and once per prewarm chunk);
    /// returning `false` aborts the render with [`SynthError::Cancelled`].
    /// It may block to implement pause, but must keep observing cancellation.
    pub fn set_render_checkpoint(&mut self, checkpoint: Option<RenderCheckpoint>) {
        self.render_checkpoint = checkpoint;
    }

    /// 设置离线渲染进度回调（`None` = 关闭）。
    ///
    /// 回调在渲染线程内被调用，必须轻量且不可阻塞；节流与 UI 映射由
    /// 调用方负责（导出侧见 `lumino-export` 的 `gpu_backend`）。
    pub fn set_render_progress(&mut self, callback: Option<RenderProgressFn>) {
        self.render_progress = callback;
    }

    /// 内部：向进度回调转发一次事件（回调为 `Fn`，可安全重复调用）。
    pub(crate) fn report_render_progress(&self, progress: RenderProgress) {
        report_progress(&self.render_progress, progress);
    }

    /// Runs the cooperative checkpoint (if any); `false` = cancelled.
    #[inline]
    pub(crate) fn check_render_checkpoint(&self) -> Result<(), SynthError> {
        checkpoint_ok(&self.render_checkpoint)
    }

    /// Returns the adapter info (for diagnostics).
    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.res.ctx.adapter_info
    }

    /// 本实例的 device 是否已丢失（REND-019 #152；离线渲染循环快速收场用）。
    #[must_use]
    pub fn is_device_lost(&self) -> bool {
        self.res.ctx.is_lost()
    }

    /// Loads a soundfont and selects `bank`/`preset`.
    ///
    /// # Errors
    ///
    /// Returns [`SynthError::SoundFont`] if parsing fails or the preset is
    /// missing.
    pub fn load_soundfont(
        &mut self,
        path: impl AsRef<std::path::Path>,
        bank: u16,
        preset: u16,
    ) -> Result<(), SynthError> {
        // REF-LGS-014：走进程级缓存——实时/导出/端口重建共用同一份解析结果。
        // 先取到新句柄再替换 `self.sf`：替换前旧核心仍存活，重建路径必命中。
        let t = std::time::Instant::now();
        let sf = SoundFont::load_cached(
            path,
            bank,
            preset,
            self.config.use_effects,
            self.config.sample_rate,
        )?;
        let shared = self
            .sf
            .as_ref()
            .is_some_and(|old| old.shares_core_with(&sf));
        tracing::info!(
            "LGS 音色库加载完成（{} ms，CPU 采样 {} B，共用已有解析: {shared}）",
            t.elapsed().as_millis(),
            sf.memory_bytes()
        );
        // REF-DATA-001：换音源 = 旧声部/旧采样池全部作废。先复位（清声部、
        // 清 sample_offsets/游标、重建 chunk），再挂新句柄——否则旧声部的
        // offset 会命中新音源的同 id 采样（潜在错音），且显存只增不减。
        if self.sf.is_some() || !self.sample_offsets.is_empty() {
            self.reset_channel_state();
            self.reset_sample_pool();
        }
        self.sf = Some(sf);
        Ok(())
    }

    /// Unloads the current soundfont.
    pub fn unload_soundfont(&mut self) {
        self.sf = None;
        self.reset_channel_state();
        self.reset_sample_pool();
    }

    // ------------------------------------------------------------------
    // Real-time event injection
    // ------------------------------------------------------------------
}
