use super::*;

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
            "voice params",
            (VoiceParams::SIZE * pool) as u64,
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
        let voice_out_buf = GrowableBuffer::with_max_capacity(
            device,
            queue,
            "voice out",
            (pool * block * 2 * 4) as u64,
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
        let voice_chans_buf = GrowableBuffer::new(
            device,
            queue,
            "voice channels",
            (pool * 4) as u64,
            wgpu::BufferUsages::STORAGE,
        );
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
        let zero = vec![0u8; VoiceParams::SIZE * pool];
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
        let zero = vec![0u8; pool * 4];
        let mut voice_chans_buf = voice_chans_buf;
        if !zero.is_empty() {
            let _ = voice_chans_buf.write(&res.ctx.device, &res.ctx.queue, 0, &zero);
        }

        let mut engine = Self {
            config,
            res,
            sf: None,
            params_buf,
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
            voice_chans_buf,
            mix_events_buf,
            mix_params_buf,
            render_bg: None,
            mix_bg: None,
            render_bg_dirty: true,
            mix_bg_dirty: true,
            channels: (0..16).map(|_| ChannelState::new()).collect(),
            voices: Vec::new(),
            key_voices: vec![VecDeque::new(); 16 * 128],
            sample_offsets: std::collections::HashMap::new(),
            samples_next_offset: 0,
            global_frame: 0,
            pending_events: VecDeque::new(),
            offline_events: Vec::new(),
            offline_cursor: 0,
            pending_mix_events: Vec::new(),
            active_voice_count: 0,
            limiter_gain: 1.0,
            limiter_tail: Vec::new(),
            last_out: None,
            last_states: None,
            prev_voice_ids: Vec::new(),
            upload_params: Vec::new(),
            upload_states: Vec::new(),
            upload_env_stages: Vec::new(),
            upload_chans: Vec::new(),
            note_counter: 0,
            voice_id_counter: 0,
            spawn_budget: [0; 16 * 128],
            active_notes: [0; 16 * 128],
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
        let sf = SoundFont::load(
            path,
            bank,
            preset,
            self.config.use_effects,
            self.config.sample_rate,
        )?;
        self.sf = Some(sf);
        Ok(())
    }

    /// Unloads the current soundfont.
    pub fn unload_soundfont(&mut self) {
        self.sf = None;
    }

    /// Returns the number of currently active voices.
    pub fn voice_count(&self) -> usize {
        self.voices.len()
    }

    /// Diagnostics: `(voices, released, ended)` - how many voices exist,
    /// how many have a release scheduled, and how many the GPU marked ended.
    #[doc(hidden)]
    pub fn debug_voice_lifecycle(&self) -> (usize, usize, usize) {
        let released = self
            .voices
            .iter()
            .filter(|v| v.released || v.release_at != u64::MAX)
            .count();
        let ended = self.voices.iter().filter(|v| v.state.ended != 0).count();
        (self.voices.len(), released, ended)
    }

    /// Diagnostics: details of the first voice's GPU state.
    #[doc(hidden)]
    pub fn debug_voice_state(&self) -> Option<(u32, u32, u32, u32, u64, u64)> {
        let v = self.voices.first()?;
        Some((
            v.state.is_released,
            v.state.ended,
            v.state.env_stage,
            v.state.env_t,
            v.release_at,
            v.start_at,
        ))
    }

    /// Diagnostics: per-voice `(key, vel, speed, amp, released, ended,
    /// env_stage, env_t, release_at, gpu_is_released, env_from)`.
    #[doc(hidden)]
    pub fn debug_voices(&self) -> Vec<VoiceDebugInfo> {
        self.voices
            .iter()
            .map(|v| {
                (
                    v.key,
                    v.vel,
                    v.speed,
                    v.amp,
                    v.released || v.release_at != u64::MAX,
                    v.state.ended != 0,
                    v.state.env_stage,
                    v.state.env_t,
                    v.release_at,
                    v.state.is_released,
                    v.state.env_from,
                )
            })
            .collect()
    }

    /// The number of frames rendered so far.
    pub fn rendered_frames(&self) -> u64 {
        self.global_frame
    }

    // ------------------------------------------------------------------
    // Real-time event injection
    // ------------------------------------------------------------------
}
