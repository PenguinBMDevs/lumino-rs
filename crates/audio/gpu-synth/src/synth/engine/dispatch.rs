use lumino_core::trf;

use super::*;

use crate::gpu::ChunkParams;

/// `wgpu::BufferSize::new` 仅对 0 字节返回 `None`；统一转成带上下文的 GPU
/// 错误，避免生产路径 `unwrap`。
fn checked_buffer_size(bytes: usize) -> Result<wgpu::BufferSize, SynthError> {
    wgpu::BufferSize::new(bytes as u64)
        .ok_or_else(|| SynthError::Gpu(trf!("buffer size 为 0（bytes={bytes}）", bytes = bytes)))
}

/// 同一次提交内所有 chunk 的 pass 派发参数（REF-LGS-002）。
#[derive(Clone, Copy)]
struct ChunkPassPlan {
    chunk_align: u32,
    voice_groups: u32,
    chunk_groups: u32,
    channels: u32,
}

impl GpuSynth {
    #[allow(clippy::modulo_one)] // STATES_SYNC_EVERY is 1; the cadence is configurable
    pub(crate) fn dispatch(&mut self, base: u64) -> Result<(), SynthError> {
        // REF-LGS-002: render/mix in frame chunks that reuse `voice_out`;
        // the buffer cap therefore scales with the chunk, not the block.
        let chunk_size = self.res.chunk_size as u32;

        let voices = self.apply_device_voice_ceiling(chunk_size)?;
        self.ensure_dispatch_buffers();

        let device = &self.res.ctx.device;
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("lumino block encoder"),
        });

        self.stage_block_uploads(&mut encoder)?;

        // REF-LGS-002: per-chunk uniforms, addressed with dynamic offsets so
        // all chunks can be recorded into one encoder/submission.
        let chunks = self.write_chunk_params(base);

        let channels = self.channels.len().max(1) as u32;
        let voice_groups = voices.div_ceil(128).max(1);
        let chunk_groups = chunk_size.div_ceil(128).max(1);
        let chunk_align = self.chunk_align as u32;
        let plan = ChunkPassPlan {
            chunk_align,
            voice_groups,
            chunk_groups,
            channels,
        };

        for i in 0..chunks {
            self.encode_chunk_passes(&mut encoder, i, &plan)?;
        }

        let (cur, states_cur) = self.encode_readback_copies(&mut encoder)?;

        let queue = &self.res.ctx.queue;
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

    /// 物理上限裁剪：受限模式报错，无限（黑乐谱）模式丢弃最老的音符组。
    fn apply_device_voice_ceiling(&mut self, chunk_size: u32) -> Result<u32, SynthError> {
        let mut voices = self.active_voice_count;
        // Physical ceiling: the voice output buffer cannot exceed the
        // device's maximum buffer size. For limited mode report a clear
        // error; for unlimited (black-MIDI) mode we trim the oldest note
        // groups to fit. With REF-LGS-002 frame chunking the ceiling is
        // `MAX / (chunk × 8)` (e.g. block 8192 / chunk 1024 -> 262k voices
        // instead of 32k before chunking), so it is far beyond any real
        // black-MIDI peak (observed ~80k).
        if (voices as u64) * (chunk_size as u64) * 8 > MAX_VOICE_OUT_BYTES {
            if self.config.max_voices == 0 {
                let max_batch = (MAX_VOICE_OUT_BYTES / (chunk_size as u64 * 8)) as u32;
                eprintln!(
                    "[warn] voices {voices} * chunk {chunk_size} exceeds device buffer ({} bytes), capping to {max_batch} (oldest trimmed)",
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
                self.retain_voices(|v| v.state.ended == 0);
                self.rebuild_key_voices();
                self.active_voice_count = max_batch;
                voices = max_batch;
            } else {
                return Err(SynthError::VoiceLimit(voices as usize));
            }
        }
        Ok(voices)
    }

    /// 按需扩容五个 per-voice 缓冲；扩容后只做一次 bind group 重建。
    fn ensure_dispatch_buffers(&mut self) {
        let voices = self.active_voice_count;
        let chunk_size = self.res.chunk_size as u32;
        // Grow the per-voice buffers if the active voice count exceeds the
        // current pool (dense MIDI may hold tens of thousands of voices).
        // Growing replaces the backing buffers, so bind groups are rebuilt
        // right below.
        if self.voice_out_buf.ensure(
            &self.res.ctx.device,
            &self.res.ctx.queue,
            (voices * chunk_size * 2 * 4) as u64,
        ) {
            self.render_bg_dirty = true;
            self.mix_bg_dirty = true;
        }
        // The per-voice GPU storage buffers (params/states/env) are written by
        // the staging belt below at `n` voices, but unlike `voice_out_buf`
        // they were only ever allocated at the pool size and never grown. If
        // the voice cap leaves slightly more than `pool` voices (it releases
        // whole note groups, so the survivor count can overshoot by a group or
        // two), the belt write overruns the fixed buffer and wgpu aborts the
        // whole submission with a validation error. Grow them here, the same
        // way the output buffer is grown. All three are bound by the render
        // bind group, so a grow dirties it for the rebuild below.
        if self.params_dyn_buf.ensure(
            &self.res.ctx.device,
            &self.res.ctx.queue,
            (std::mem::size_of::<VoiceParamsDyn>() * voices as usize) as u64,
        ) {
            self.render_bg_dirty = true;
        }
        // REF-LGS-010: 静态表按槽位高水位扩容。`GrowableBuffer::ensure` 会
        // 清零新区域并保留旧内容，未分配槽位永远不被读取（只在分配时写入）。
        if self.params_static_buf.ensure(
            &self.res.ctx.device,
            &self.res.ctx.queue,
            (std::mem::size_of::<VoiceParamsStatic>() * self.slot_count as usize) as u64,
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
            (std::mem::size_of::<EnvStageGpu>() * self.env_pool.stages().len().max(1)) as u64,
        ) {
            self.render_bg_dirty = true;
        }
        if self.render_bg_dirty {
            self.rebuild_bind_groups();
        }
        if self.mix_bg_dirty {
            self.rebuild_mix_bind_group();
        }
    }

    /// 把本块上传写入 staging belt，并在返回前且仅在返回前 `finish` 一次。
    fn stage_block_uploads(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<(), SynthError> {
        // Voice parameter uploads, staged through the persistent belt into
        // the SAME submission as the compute passes (one submit per block).
        {
            let n = (self.active_voice_count as usize)
                .min(self.upload_params.len())
                .max(1);
            self.belt
                .write_buffer(
                    encoder,
                    self.params_dyn_buf.buffer(),
                    0,
                    checked_buffer_size(std::mem::size_of::<VoiceParamsDyn>() * n)?,
                )
                .copy_from_slice(bytemuck::cast_slice(&self.upload_params[..n]));
            self.belt
                .write_buffer(
                    encoder,
                    self.states_buf.buffer(),
                    0,
                    checked_buffer_size(std::mem::size_of::<VoiceState>() * n)?,
                )
                .copy_from_slice(bytemuck::cast_slice(&self.upload_states[..n]));
            // REF-LGS-010: 只上传本块新分配/改写的静态槽位（连续区间合并后
            // 一次 belt 写；已驻留的声部零重传）。
            for &(start, len) in &self.static_runs {
                let bytes = bytemuck::cast_slice::<VoiceParamsStatic, u8>(
                    &self.static_table[start as usize..(start + len) as usize],
                );
                self.belt
                    .write_buffer(
                        encoder,
                        self.params_static_buf.buffer(),
                        u64::from(start) * VoiceParamsStatic::SIZE as u64,
                        checked_buffer_size(bytes.len())?,
                    )
                    .copy_from_slice(bytes);
            }
            // REF-LGS-011: 只追加上一块以来新增的包络模板（稳态零上传）。
            let env_len = self.env_pool.pending().len();
            if env_len > 0 {
                let env_start = self.env_pool.stages().len() - env_len;
                let bytes = bytemuck::cast_slice(&self.env_pool.stages()[env_start..]);
                self.belt
                    .write_buffer(
                        encoder,
                        self.env_buf.buffer(),
                        env_start as u64 * EnvStageGpu::SIZE as u64,
                        checked_buffer_size(bytes.len())?,
                    )
                    .copy_from_slice(bytes);
                self.env_pool.mark_uploaded();
            }
            self.belt.finish();
        }
        Ok(())
    }

    /// 填充 chunk uniform 并一次写入 GPU buffer；返回 chunk 数供主函数派发。
    fn write_chunk_params(&mut self, base: u64) -> u32 {
        let block = self.config.block_size as u32;
        let chunk_size = self.res.chunk_size as u32;
        let chunks = block.div_ceil(chunk_size).max(1);
        let queue = &self.res.ctx.queue;
        {
            let align = self.chunk_align as usize;
            self.chunk_params_bytes.clear();
            self.chunk_params_bytes.resize(chunks as usize * align, 0);
            for (i, slot) in self.chunk_params_bytes.chunks_mut(align).enumerate() {
                let p = ChunkParams {
                    render_frame_base: (base + i as u64 * chunk_size as u64) as u32,
                    mix_frame_base: (i as u64 * chunk_size as u64) as u32,
                    chunk_size,
                    _pad: 0,
                };
                slot[..ChunkParams::SIZE].copy_from_slice(bytemuck::bytes_of(&p));
            }
            queue.write_buffer(&self.chunk_params_buf, 0, &self.chunk_params_bytes);
        }
        chunks
    }

    /// 单个 chunk 的 render -> mix -> mix_reduce 三个 pass，顺序固定。
    fn encode_chunk_passes(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        i: u32,
        plan: &ChunkPassPlan,
    ) -> Result<(), SynthError> {
        let render_bg = self
            .render_bg
            .as_ref()
            .ok_or_else(|| SynthError::Gpu("render bind group missing".into()))?;
        let mix_bg = self
            .mix_bg
            .as_ref()
            .ok_or_else(|| SynthError::Gpu("mix bind group missing".into()))?;

        let ChunkPassPlan {
            chunk_align,
            voice_groups,
            chunk_groups,
            channels,
            ..
        } = *plan;
        let offset = i * chunk_align;

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("render pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.res.render_pipeline);
            pass.set_bind_group(0, render_bg, &[offset]);
            // REF-LGS-005: one thread per voice renders the whole chunk;
            // the old gid.y segment split + fast-forward is gone.
            pass.dispatch_workgroups(voice_groups, 1, 1);
        }

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("mix pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.res.mix_pipeline);
            pass.set_bind_group(0, mix_bg, &[offset]);
            pass.dispatch_workgroups(chunk_groups, channels, 1);
        }

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("mix reduce pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.res.mix_reduce_pipeline);
            pass.set_bind_group(0, mix_bg, &[offset]);
            pass.dispatch_workgroups(chunk_groups, 1, 1);
        }
        Ok(())
    }

    /// 所有 pass 之后追加回读拷贝，返回本块落地的 (out_slot, states_slot)。
    fn encode_readback_copies(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<(usize, usize), SynthError> {
        let device = &self.res.ctx.device;
        let queue = &self.res.ctx.queue;
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
        Ok((cur, states_cur))
    }
}
