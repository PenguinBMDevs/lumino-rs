use super::*;

impl GpuSynth {
    pub(crate) fn upload_voices(&mut self, base: u64) -> Result<(), SynthError> {
        self.trim_voice_pool_for_block();
        let n = self.voices.len();
        // Reuse the per-block upload buffers: `resize` keeps the allocation,
        // so a cap-sized pool does not re-allocate ~1.5 MB every block.
        self.upload_params.resize(n.max(1), VoiceParams::zeroed());
        self.upload_states.resize(n.max(1), VoiceState::zeroed());
        // Pre-compute per-voice env stage counts and prefix sums so the
        // env upload can be parallelized (each voice knows its base).
        let env_counts: Vec<u32> = self
            .voices
            .iter()
            .map(|v| {
                if v.fade_out {
                    1
                } else {
                    v.env_stages.len() as u32
                }
            })
            .collect();
        let mut env_bases: Vec<u32> = Vec::with_capacity(n);
        let mut total_env: usize = 0;
        for &c in &env_counts {
            env_bases.push(total_env as u32);
            total_env += c as usize;
        }
        self.upload_env_stages
            .resize(total_env.max(1), EnvStageGpu::zeroed());
        self.upload_chans.resize(n.max(1), 0);

        // Snapshot data needed for the parallel phase to avoid borrowing
        // `self` inside the closure.
        let sample_offsets = &self.sample_offsets;
        let prev_ids = self.prev_voice_ids.clone();
        let new_ids: Vec<u32> = self.voices.iter().map(|v| v.id).collect();
        let last_states = self.last_states.clone();
        let st_count = last_states
            .as_ref()
            .map_or(0, |st| st.len() / VoiceState::SIZE);
        let base_frame = base;
        let interp = self.config.interpolation;
        let sr = self.config.sample_rate;

        // Parallel path for large voice counts (black MIDI peaks). For small
        // n the rayon overhead outweighs the benefit, so keep the sequential
        // fast path.
        if n > 2048 {
            // Pre-fetch sample offsets and update per-voice `sample_offset_r`
            // in a first pass (hash lookups are not thread-safe for mutation,
            // so do them sequentially but cheap). Also fill env stages
            // sequentially (variable-length slices are not easily parallelized
            // without disjoint borrow issues).
            let mut sample_offs: Vec<(u32, u32)> = Vec::with_capacity(n);
            for (i, v) in self.voices.iter_mut().enumerate() {
                let off = sample_offsets
                    .get(&v.sample_id)
                    .map(|(o, _)| *o)
                    .unwrap_or(0);
                let off_r = sample_offsets
                    .get(&v.sample_id_r)
                    .map(|(o, _)| *o)
                    .unwrap_or(off);
                v.sample_offset_r = off_r;
                sample_offs.push((off, off_r));
                // Fill env stages for this voice (sequential, cheap).
                let env_base = env_bases[i] as usize;
                let env_count = env_counts[i] as usize;
                let slice = &mut self.upload_env_stages[env_base..env_base + env_count];
                if v.fade_out {
                    slice[0] = EnvStageGpu {
                        kind: 0,
                        target_val: 0.0,
                        duration: (sr / 1000).max(1),
                    };
                } else {
                    for (j, s) in v.env_stages.iter().enumerate() {
                        slice[j] = EnvStageGpu {
                            kind: s.kind,
                            target_val: s.target,
                            duration: s.duration,
                        };
                    }
                }
            }
            // Parallel generation of params/chans/states via owned Vecs to avoid
            // borrow checker issues with &mut self in rayon closures. The
            // disjoint slices are filled sequentially after the parallel map.
            let voices_ref = &self.voices;
            let prev_ids_ref = &prev_ids;
            let last_states_ref = &last_states;
            let env_bases_ref = &env_bases;
            let sample_offs_ref = &sample_offs;
            let ((params_vec, chans_vec), states_vec) = rayon::join(
                || {
                    rayon::join(
                        || {
                            (0..n)
                                .into_par_iter()
                                .map(|i| {
                                    let v = &voices_ref[i];
                                    let (off, off_r) = sample_offs_ref[i];
                                    let env_base = env_bases_ref[i];
                                    let mut p =
                                        v.gpu_params(off, off_r, env_base, base_frame, interp);
                                    if v.fade_out {
                                        p.env_count = 1;
                                        p.release_idx = 0;
                                        p.finished_idx = 1;
                                    }
                                    p
                                })
                                .collect::<Vec<_>>()
                        },
                        || {
                            (0..n)
                                .into_par_iter()
                                .map(|i| {
                                    let v = &voices_ref[i];
                                    v.channel as u32
                                        | ((if v.released || v.release_at != u64::MAX {
                                            1u32
                                        } else {
                                            0u32
                                        }) << 7)
                                })
                                .collect::<Vec<_>>()
                        },
                    )
                },
                || {
                    (0..n)
                        .into_par_iter()
                        .map(|i| {
                            let v = &voices_ref[i];
                            let resumed = if v.state.ended != 0 {
                                None
                            } else {
                                match prev_ids_ref.binary_search(&v.id) {
                                    Ok(k) if k < st_count => {
                                        if let Some(buf) = last_states_ref.as_ref() {
                                            let off = k * VoiceState::SIZE;
                                            Some(*bytemuck::from_bytes::<VoiceState>(
                                                &buf[off..off + VoiceState::SIZE],
                                            ))
                                        } else {
                                            None
                                        }
                                    }
                                    _ => None,
                                }
                            };
                            if v.state.ended != 0 {
                                v.state
                            } else {
                                resumed.unwrap_or(v.state)
                            }
                        })
                        .collect::<Vec<_>>()
                },
            );
            self.upload_params[..n].copy_from_slice(&params_vec);
            self.upload_chans[..n].copy_from_slice(&chans_vec);
            self.upload_states[..n].copy_from_slice(&states_vec);
        } else {
            let params = &mut self.upload_params;
            let states = &mut self.upload_states;
            let chans = &mut self.upload_chans;
            let env_stages = &mut self.upload_env_stages;
            let prev_ids_ref = &prev_ids;
            let mut k = 0usize;
            for (i, v) in self.voices.iter_mut().enumerate() {
                let sample_offset = sample_offsets
                    .get(&v.sample_id)
                    .map(|(off, _)| *off)
                    .unwrap_or(0);
                let sample_offset_r = sample_offsets
                    .get(&v.sample_id_r)
                    .map(|(off, _)| *off)
                    .unwrap_or(sample_offset);
                v.sample_offset_r = sample_offset_r;
                let env_base = env_bases[i];
                let env_count = env_counts[i];
                let slice = &mut env_stages[env_base as usize..(env_base + env_count) as usize];
                if v.fade_out {
                    slice[0] = EnvStageGpu {
                        kind: 0,
                        target_val: 0.0,
                        duration: (sr / 1000).max(1),
                    };
                } else {
                    for (j, s) in v.env_stages.iter().enumerate() {
                        slice[j] = EnvStageGpu {
                            kind: s.kind,
                            target_val: s.target,
                            duration: s.duration,
                        };
                    }
                }
                let mut gp =
                    v.gpu_params(sample_offset, sample_offset_r, env_base, base_frame, interp);
                if v.fade_out {
                    gp.env_count = 1;
                    gp.release_idx = 0;
                    gp.finished_idx = 1;
                }
                params[i] = gp;
                while k < prev_ids_ref.len() && prev_ids_ref[k] < v.id {
                    k += 1;
                }
                let resumed = match last_states.as_ref() {
                    Some(st)
                        if k < prev_ids_ref.len() && prev_ids_ref[k] == v.id && k < st_count =>
                    {
                        let off = k * VoiceState::SIZE;
                        Some(*bytemuck::from_bytes::<VoiceState>(
                            &st[off..off + VoiceState::SIZE],
                        ))
                    }
                    _ => None,
                };
                states[i] = if v.state.ended != 0 {
                    v.state
                } else {
                    resumed.unwrap_or(v.state)
                };
                chans[i] = v.channel as u32
                    | ((if v.released || v.release_at != u64::MAX {
                        1u32
                    } else {
                        0u32
                    }) << 7);
            }
        }
        self.prev_voice_ids = new_ids;
        // (Upload is deferred to `dispatch` so all GPU work - staging copies,
        // render pass, readback copies - happens in ONE submit; separate
        // submits measured ~9ms each of fixed wgpu/Vulkan overhead.)
        self.active_voice_count = n as u32;
        Ok(())
    }
}
