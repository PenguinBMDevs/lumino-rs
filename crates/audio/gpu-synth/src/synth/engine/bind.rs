use super::*;

impl GpuSynth {
    pub(crate) fn update_mix_params(&mut self, base: u64) -> Result<(), SynthError> {
        let queue = &self.res.ctx.queue;
        let block = self.config.block_size as u32;
        let end = base + block as u64;
        let sr = self.config.sample_rate;

        // Take this block's deferred controller events; keep the rest for
        // the blocks that follow.
        let mut in_block: Vec<(u64, u8, u8, u8)> = Vec::new();
        let mut rest: Vec<(u64, u8, u8, u8)> = Vec::new();
        for ev in std::mem::take(&mut self.pending_mix_events) {
            if ev.0 < end {
                in_block.push(ev);
            } else {
                rest.push(ev);
            }
        }
        self.pending_mix_events = rest;
        in_block.sort_by_key(|e| e.0);

        // Frame-exact controller curve: the mix kernel replays this block's
        // events against the block-start lerp states, so the output does not
        // depend on the block size or on how many events a block contains.
        let events: Vec<MixEvent> = in_block
            .iter()
            .map(|e| MixEvent {
                frame: (e.0 - base) as u32,
                channel: e.1 as u32,
                cc: e.2 as u32,
                value: e.3 as f32 / 128.0,
            })
            .collect();

        // Per-channel block-start states, then advance the CPU-side lerp
        // state machines through this block (all events + the block end) so
        // the next block starts from the right values.
        let mut starts: Vec<MixStart> = Vec::with_capacity(MIX_CHANNELS);
        for ch_idx in 0..MIX_CHANNELS {
            let st = &mut self.channels[ch_idx];
            starts.push(MixStart {
                vol: st.volume.current,
                vol_step: st.volume.step,
                vol_end: st.volume.end,
                expr: st.expression.current,
                expr_step: st.expression.step,
                expr_end: st.expression.end,
                pan: st.pan.current,
                pan_step: st.pan.step,
                pan_end: st.pan.end,
                _pad: [0.0; 3],
            });
            for ev in in_block.iter().filter(|e| e.1 as usize == ch_idx) {
                let (s, cc, value) = (ev.0, ev.2, ev.3);
                match cc {
                    0x07 => {
                        st.volume.advance_to(s);
                        st.volume.set_end(value as f32 / 128.0, sr);
                    }
                    0x0B => {
                        st.expression.advance_to(s);
                        st.expression.set_end(value as f32 / 128.0, sr);
                    }
                    0x0A | 0x08 => {
                        st.pan.advance_to(s);
                        st.pan.set_end(value as f32 / 128.0, sr);
                    }
                    _ => {}
                }
            }
            st.volume.advance_to(end);
            st.expression.advance_to(end);
            st.pan.advance_to(end);
        }

        let device = &self.res.ctx.device;
        if self
            .mix_events_buf
            .write(device, queue, 0, bytemuck::cast_slice(&events))?
        {
            self.mix_bg_dirty = true;
        }
        let params = MixParams {
            voice_count: self.active_voice_count,
            block_size: block,
            channel_count: MIX_CHANNELS as u32,
            event_count: events.len() as u32,
            lerp_len: sr as f32 * 0.01,
            _pad: [0.0; 3],
            starts: starts
                .try_into()
                .map_err(|_| SynthError::Gpu("channel count mismatch".into()))?,
        };
        if std::env::var("LUMINO_VOICEDUMP").is_ok() && base > 415_000 && base < 420_000 {
            let s = &self.channels[0];
            eprintln!(
                "[mix] base={base} ch0 vol={:.4} expr={:.4} pan={:.4}",
                s.volume.current, s.expression.current, s.pan.current
            );
        }
        queue.write_buffer(&self.mix_params_buf, 0, bytemuck::cast_slice(&[params]));
        Ok(())
    }

    pub(crate) fn rebuild_bind_groups(&mut self) {
        let device = &self.res.ctx.device;
        let mut entries: Vec<wgpu::BindGroupEntry> = Vec::with_capacity(13);
        entries.push(wgpu::BindGroupEntry {
            binding: 0,
            resource: self.params_buf.buffer().as_entire_binding(),
        });
        for (i, chunk) in self.samples_chunks.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: SAMPLES_CHUNK_BINDING_BASE + i as u32,
                resource: chunk.buffer().as_entire_binding(),
            });
        }
        entries.extend([
            wgpu::BindGroupEntry {
                binding: crate::gpu::SINC_BINDING,
                resource: self.sinc_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: crate::gpu::ENV_BINDING,
                resource: self.env_buf.buffer().as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: crate::gpu::STATES_BINDING,
                resource: self.states_buf.buffer().as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: crate::gpu::VOICE_OUT_BINDING,
                resource: self.voice_out_buf.buffer().as_entire_binding(),
            },
        ]);
        self.render_bg = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("render bind group"),
            layout: &self.res.render_layout,
            entries: &entries,
        }));
        self.mix_bg = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mix bind group"),
            layout: &self.res.mix_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.voice_out_buf.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.out_storage_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.voice_chans_buf.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.mix_events_buf.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.mix_params_buf.as_entire_binding(),
                },
            ],
        }));
        self.render_bg_dirty = false;
        self.mix_bg_dirty = false;
    }

    pub(crate) fn rebuild_mix_bind_group(&mut self) {
        let device = &self.res.ctx.device;
        self.mix_bg = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mix bind group"),
            layout: &self.res.mix_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.voice_out_buf.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.out_storage_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.voice_chans_buf.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.mix_events_buf.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.mix_params_buf.as_entire_binding(),
                },
            ],
        }));
        self.mix_bg_dirty = false;
    }
}
