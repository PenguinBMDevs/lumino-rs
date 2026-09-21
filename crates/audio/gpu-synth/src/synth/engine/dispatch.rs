use super::*;

impl GpuSynth {
    #[allow(clippy::modulo_one)] // STATES_SYNC_EVERY is 1; the cadence is configurable
    pub(crate) fn dispatch(&mut self, _base: u64) -> Result<(), SynthError> {
        let mut voices = self.active_voice_count;
        let block = self.config.block_size as u32;

        // Physical ceiling: the voice output buffer cannot exceed the
        // device's maximum buffer size. For limited mode report a clear
        // error; for unlimited (black-MIDI) chunking would be required to
        // exceed this, but the limit is ~524k voices at block 512 (~131k at
        // 2048), far beyond any real black MIDI peak (observed ~80k), so a
        // trim-to-fit fallback is practically unlimited and keeps the code
        // simple. A full chunked dispatch is reserved for a future change if
        // a file truly needs >500k simultaneous voices.
        if (voices as u64) * (block as u64) * 8 > MAX_VOICE_OUT_BYTES {
            if self.config.max_voices == 0 {
                let max_batch = (MAX_VOICE_OUT_BYTES / (block as u64 * 8)) as u32;
                eprintln!(
                    "[warn] voices {voices} * block {block} exceeds device buffer ({} bytes), capping to {max_batch} (oldest trimmed)",
                    MAX_VOICE_OUT_BYTES
                );
                // Trim oldest voices down to max_batch (same steal semantics
                // as the global cap, but at the device limit).
                let over = (voices - max_batch) as usize;
                // Group by note and keep newest (oldest trimmed).
                let mut groups: Vec<(u64, u8, u64, Vec<usize>)> = Vec::new();
                for (i, v) in self.voices.iter().enumerate() {
                    match groups.last_mut() {
                        Some((_, _, nid, g)) if *nid == v.note_id => g.push(i),
                        _ => groups.push((v.spawn_frame, v.vel, v.note_id, vec![i])),
                    }
                }
                groups.sort_by_key(|&(spawn, vel, _, _)| (spawn, vel));
                let mut freed = 0usize;
                for (_, _, _, positions) in &groups {
                    if freed >= over {
                        break;
                    }
                    for &i in positions {
                        let v = &mut self.voices[i];
                        if v.state.ended == 0 {
                            v.state.ended = 1;
                            freed += 1;
                            if freed >= over {
                                break;
                            }
                        }
                    }
                }
                self.voices.retain(|v| v.state.ended == 0);
                self.rebuild_key_voices();
                self.active_voice_count = max_batch;
                voices = max_batch;
            } else {
                return Err(SynthError::VoiceLimit(voices as usize));
            }
        }

        // Grow the per-voice buffers if the active voice count exceeds the
        // current pool (dense MIDI may hold tens of thousands of voices).
        // Growing replaces the backing buffers, so bind groups are rebuilt
        // right below.
        if self.voice_out_buf.ensure(
            &self.res.ctx.device,
            &self.res.ctx.queue,
            (voices * block * 2 * 4) as u64,
        ) {
            self.render_bg_dirty = true;
            self.mix_bg_dirty = true;
        }
        if self.voice_chans_buf.ensure(
            &self.res.ctx.device,
            &self.res.ctx.queue,
            (voices * 4) as u64,
        ) {
            self.render_bg_dirty = true;
            self.mix_bg_dirty = true;
        }
        // The per-voice GPU storage buffers (params/states/env) are written by
        // the staging belt below at `n` voices, but unlike `voice_out_buf` /
        // `voice_chans_buf` they were only ever allocated at the pool size and
        // never grown. If the voice cap leaves slightly more than `pool` voices
        // (it releases whole note groups, so the survivor count can overshoot
        // by a group or two), the belt write overruns the fixed buffer and
        // wgpu aborts the whole submission with a validation error. Grow them
        // here, the same way the output buffers are grown. All three are bound
        // by the render bind group, so a grow dirties it for the rebuild below.
        if self.params_buf.ensure(
            &self.res.ctx.device,
            &self.res.ctx.queue,
            (std::mem::size_of::<VoiceParams>() * voices as usize) as u64,
        ) {
            self.render_bg_dirty = true;
        }
        if self.states_buf.ensure(
            &self.res.ctx.device,
            &self.res.ctx.queue,
            (std::mem::size_of::<VoiceState>() * voices as usize) as u64,
        ) {
            self.render_bg_dirty = true;
        }
        if self.env_buf.ensure(
            &self.res.ctx.device,
            &self.res.ctx.queue,
            (std::mem::size_of::<EnvStageGpu>() * self.upload_env_stages.len().max(1)) as u64,
        ) {
            self.render_bg_dirty = true;
        }
        if self.render_bg_dirty {
            self.rebuild_bind_groups();
        }
        if self.mix_bg_dirty {
            self.rebuild_mix_bind_group();
        }

        let device = &self.res.ctx.device;
        let queue = &self.res.ctx.queue;
        let block = self.config.block_size as u32;
        let voices = self.active_voice_count;

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("lumino block encoder"),
        });

        // Voice parameter uploads, staged through the persistent belt into
        // the SAME submission as the compute passes (one submit per block).
        {
            let n = (self.active_voice_count as usize)
                .min(self.upload_params.len())
                .max(1);
            self.belt
                .write_buffer(
                    &mut encoder,
                    self.params_buf.buffer(),
                    0,
                    wgpu::BufferSize::new((std::mem::size_of::<VoiceParams>() * n) as u64).unwrap(),
                    device,
                )
                .copy_from_slice(bytemuck::cast_slice(&self.upload_params[..n]));
            self.belt
                .write_buffer(
                    &mut encoder,
                    self.states_buf.buffer(),
                    0,
                    wgpu::BufferSize::new((std::mem::size_of::<VoiceState>() * n) as u64).unwrap(),
                    device,
                )
                .copy_from_slice(bytemuck::cast_slice(&self.upload_states[..n]));
            if !self.upload_env_stages.is_empty() {
                self.belt
                    .write_buffer(
                        &mut encoder,
                        self.env_buf.buffer(),
                        0,
                        wgpu::BufferSize::new(
                            (std::mem::size_of::<EnvStageGpu>() * self.upload_env_stages.len())
                                as u64,
                        )
                        .unwrap(),
                        device,
                    )
                    .copy_from_slice(bytemuck::cast_slice(&self.upload_env_stages));
            }
            self.belt
                .write_buffer(
                    &mut encoder,
                    self.voice_chans_buf.buffer(),
                    0,
                    wgpu::BufferSize::new((4 * n) as u64).unwrap(),
                    device,
                )
                .copy_from_slice(bytemuck::cast_slice(&self.upload_chans[..n]));
            self.belt.finish();
        }

        let render_bg = self
            .render_bg
            .as_ref()
            .ok_or_else(|| SynthError::Gpu("render bind group missing".into()))?;
        let mix_bg = self
            .mix_bg
            .as_ref()
            .ok_or_else(|| SynthError::Gpu("mix bind group missing".into()))?;

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("render pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.res.render_pipeline);
            pass.set_bind_group(0, render_bg, &[]);
            // Each voice is split across RENDER_SEGMENTS threads (gid.y);
            // the shader fast-forwards to its segment start, so the GPU
            // parallelism is voices x segments.
            pass.dispatch_workgroups(voices.div_ceil(128).max(1), crate::gpu::RENDER_SEGMENTS, 1);
        }

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("mix pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.res.mix_pipeline);
            pass.set_bind_group(0, mix_bg, &[]);
            pass.dispatch_workgroups(block.div_ceil(128).max(1), 1, 1);
        }

        // Readbacks. The voice states are only copied back every
        // STATES_SYNC_EVERY blocks (see `states_sync_counter`); the output
        // must come back every block.
        let cur = self.out_readback_cur;
        encoder.copy_buffer_to_buffer(
            &self.out_storage_buf,
            0,
            &self.out_readback[cur],
            0,
            (self.config.block_size * 2 * 4) as u64,
        );
        let states_cur = self.states_readback_cur;
        if self.states_sync_counter == 0 {
            let states_bytes = (VoiceState::SIZE * self.voices.len()) as u64;
            let grew = self.states_readback[states_cur].ensure(device, queue, states_bytes);
            if grew {
                // The readback buffer was replaced; nothing else references
                // it (it is mapped below by value), so no rebind is needed.
            }
            encoder.copy_buffer_to_buffer(
                self.states_buf.buffer(),
                0,
                self.states_readback[states_cur].buffer(),
                0,
                states_bytes,
            );
        }

        let idx = queue.submit(Some(encoder.finish()));

        // One-block pipeline: this submission's readback is consumed by the
        // next `render_block` (`collect_pending_readback`), so the GPU runs
        // this block while the CPU maps the PREVIOUS block back - the
        // per-block synchronous poll wait is gone. Record the exact slots
        // the copies landed in: they are read back next block, and silent
        // blocks that skip dispatching never shift this window.
        self.pending = Some(PendingReadback {
            idx,
            out_slot: cur,
            states_slot: states_cur,
        });
        // `STATES_SYNC_EVERY = 1` keeps the counter at 0 (every block maps
        // its states); the modulo is intentional and kept for the
        // configurable cadence.
        self.states_sync_counter = if STATES_SYNC_EVERY > 1 {
            (self.states_sync_counter + 1) % STATES_SYNC_EVERY
        } else {
            0
        };
        Ok(())
    }
}
