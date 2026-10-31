use super::*;

impl GpuSynth {
    pub(crate) fn upload_voices(&mut self, _base: u64) -> Result<(), SynthError> {
        self.trim_voice_pool_for_block();
        let n = self.voices.len();
        // REF-LGS-010: 新声部分配静态槽位。静态参数只在分配槽位这一块写一次
        // GPU（压缩声部列表不改变槽位），之后每块只上传动态字段。
        let interp = self.config.interpolation;
        self.assign_static_slots(interp);
        self.coalesce_dirty_slots();
        // REF-LGS-011: 相同 stage 序列共享同一条包络模板。
        self.intern_env_templates();

        // Reuse the per-block upload buffers: `resize` keeps the allocation,
        // so a cap-sized pool does not re-allocate ~1.5 MB every block.
        self.upload_params
            .resize(n.max(1), VoiceParamsDyn::zeroed());
        self.upload_states.resize(n.max(1), VoiceState::zeroed());
        self.upload_chans.resize(n.max(1), 0);

        // Snapshot data needed for the parallel phase to avoid borrowing
        // `self` inside the closure. PREF-006 B：`prev_voice_ids` 与
        // `last_states` 用 take/交换代替 clone（每块省 O(voices) 的 memcpy；
        // `last_states` 用完原样放回，供下一块 readback 的 resume 匹配）。
        let sample_offsets = &self.sample_offsets;
        let prev_ids = std::mem::take(&mut self.prev_voice_ids);
        let mut new_ids = std::mem::take(&mut self.scratch_voice_ids);
        new_ids.clear();
        new_ids.extend(self.voices.iter().map(|v| v.id));
        let last_states = self.last_states.take();
        let st_count = last_states
            .as_ref()
            .map_or(0, |st| st.len() / VoiceState::SIZE);
        let fade_env_base = self.fade_env_template;

        // Parallel path for large voice counts (black MIDI peaks). For small
        // n the rayon overhead outweighs the benefit, so keep the sequential
        // fast path.
        if n > 2048 {
            // Resolve the per-voice sample offsets first (hash lookups are
            // not thread-safe for mutation, so do them sequentially but cheap).
            let mut sample_offs: Vec<(u32, u32)> = Vec::with_capacity(n);
            for v in self.voices.iter_mut() {
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
            }
            // Parallel generation of params/chans/states via owned Vecs to avoid
            // borrow checker issues with &mut self in rayon closures. The
            // disjoint slices are filled sequentially after the parallel map.
            let voices_ref = &self.voices;
            let prev_ids_ref = &prev_ids;
            let last_states_ref = &last_states;
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
                                    let env_base = if v.fade_out {
                                        fade_env_base
                                    } else {
                                        v.env_template
                                    };
                                    v.gpu_dyn(v.slot, off, off_r, env_base, v.fade_out)
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
                                        }) << 8)
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
                let env_base = if v.fade_out {
                    fade_env_base
                } else {
                    v.env_template
                };
                params[i] = v.gpu_dyn(v.slot, sample_offset, sample_offset_r, env_base, v.fade_out);
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
                    }) << 8);
            }
        }
        self.prev_voice_ids = new_ids;
        // 旧列表缓冲转为下一块的 scratch（交换复用，零分配）
        self.scratch_voice_ids = prev_ids;
        self.last_states = last_states;
        // (Upload is deferred to `dispatch` so all GPU work - staging copies,
        // render pass, readback copies - happens in ONE submit; separate
        // submits measured ~9ms each of fixed wgpu/Vulkan overhead.)
        self.active_voice_count = n as u32;
        // REF-LGS-001: 构建稳定的 (通道, released) 声部桶。索引按 v 升序
        // 追加，与旧 mix 内核的累加顺序一致，因此浮点结果逐位不变。
        build_mix_buckets(
            &self.upload_chans[..n],
            self.channels.len(),
            &mut self.mix_bucket_offsets,
            &mut self.mix_bucket_cursor,
            &mut self.mix_bucket_indices,
        );
        Ok(())
    }

    /// REF-LGS-011: 把每个声部的 stage 序列驻留到共享模板池。
    ///
    /// 只有 `env_template == NO_ENV_TEMPLATE` 的声部（新出生 / CC72/73 重算过
    /// 包络）会做哈希查表；已驻留的声部每块零成本。
    fn intern_env_templates(&mut self) {
        let sample_rate = self.config.sample_rate;
        let Self {
            voices,
            env_pool,
            fade_env_template,
            ..
        } = self;
        if *fade_env_template == u32::MAX {
            *fade_env_template = env_pool.intern_fade(sample_rate);
        }
        for v in voices.iter_mut() {
            if v.env_template == NO_ENV_TEMPLATE {
                v.env_template = env_pool.intern(&v.env_stages);
            }
        }
    }
}

/// 构建稳定的 (通道, released) 声部桶（REF-LGS-001）。
///
/// `chans[v] = channel | released << 8`；每组内的索引按 `v` 升序追加，
/// 保证 mix 内核按原顺序累加（浮点逐位一致）。`offsets` 布局：
/// `[sustained: 0..=channels] ++ [released: 0..=channels]`。
fn build_mix_buckets(
    chans: &[u32],
    channels: usize,
    offsets: &mut Vec<u32>,
    cursor: &mut Vec<u32>,
    indices: &mut Vec<u32>,
) {
    let stride = channels + 1;
    offsets.clear();
    offsets.resize(2 * stride, 0);
    for &vc in chans {
        let ch = (vc & 0xFF) as usize;
        let rel = ((vc >> 8) & 1) as usize;
        if ch < channels {
            offsets[rel * stride + ch + 1] += 1;
        }
    }
    for group in 0..2 {
        let base = group * stride;
        for c in 0..channels {
            offsets[base + c + 1] += offsets[base + c];
        }
    }
    // 第二组（released）从 sustained 总数之后开始，避免两组索引区间重叠。
    let sustained_total = offsets[channels];
    for c in 0..=channels {
        offsets[stride + c] += sustained_total;
    }
    cursor.clear();
    cursor.extend_from_slice(offsets);
    indices.clear();
    indices.resize(chans.len(), 0);
    for (v, &vc) in chans.iter().enumerate() {
        let ch = (vc & 0xFF) as usize;
        let rel = ((vc >> 8) & 1) as usize;
        if ch >= channels {
            continue;
        }
        let slot = rel * stride + ch;
        let pos = cursor[slot] as usize;
        indices[pos] = v as u32;
        cursor[slot] += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::build_mix_buckets;

    /// 桶必须保持每个 (通道, group) 内的 v 升序，否则浮点求和顺序会变。
    #[test]
    fn mix_buckets_preserve_voice_order_and_split_released() {
        // v: 0 ch0 sustained, 1 ch1 sustained, 2 ch0 sustained,
        //    3 ch1 released, 4 ch0 released
        let chans = [0u32, 1, 0, 1 | (1 << 8), 1 << 8];
        let mut offsets = Vec::new();
        let mut cursor = Vec::new();
        let mut indices = Vec::new();
        build_mix_buckets(&chans, 2, &mut offsets, &mut cursor, &mut indices);

        let stride = 3;
        // sustained: ch0 = [0, 2], ch1 = [1]
        assert_eq!(&offsets[0..stride], &[0, 2, 3]);
        // released: ch0 = [4], ch1 = [3]，从 sustained 总数 3 之后开始
        assert_eq!(&offsets[stride..], &[3, 4, 5]);
        assert_eq!(indices, vec![0, 2, 1, 4, 3]);
    }

    /// 越界通道（配置空间小于文件端口上界）不得进入桶，也不得 panic。
    #[test]
    fn mix_buckets_skip_out_of_range_channels() {
        let chans = [0u32, 16, 1 | (1 << 8)];
        let mut offsets = Vec::new();
        let mut cursor = Vec::new();
        let mut indices = Vec::new();
        build_mix_buckets(&chans, 2, &mut offsets, &mut cursor, &mut indices);

        let stride = 3;
        assert_eq!(&offsets[0..stride], &[0, 1, 1]);
        assert_eq!(&offsets[stride..], &[1, 1, 2]);
        // 索引仍是原始 v（v1 越界被跳过，其槽位保留 0 占位）
        assert_eq!(indices[0], 0);
        assert_eq!(indices[1], 2);
        assert_eq!(indices[2], 0);
    }
}
