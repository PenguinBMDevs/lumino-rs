use super::*;

impl GpuSynth {
    /// REF-LGS-007：把一个**已打包**的采样写入 GPU 池，并登记偏移 + 增益。
    ///
    /// 打包（量化）在调用方的 rayon 并行段完成——大音源（数百 MB）的量化是
    /// 一次性的 CPU 成本，串行做会拖慢短渲染（实测 10 s 素材 −3.5%）。
    ///
    /// `offset` 以 **f32 采样**为单位（shader 用 `i >> 1` 定位 u32 字），并保持
    /// 偶数对齐——一个 u32 装两帧，奇数起点会跨字。`len` 是原始采样帧数。
    pub(crate) fn upload_sample_packed(
        &mut self,
        sample_id: usize,
        len: u32,
        words: &[u32],
        gain: f32,
    ) -> Result<bool, SynthError> {
        let mut offset = self.samples_next_offset;
        if offset & 1 == 1 {
            offset += 1;
        }
        let grown = write_samples(
            &mut self.samples_chunks,
            &self.res.ctx.device,
            &self.res.ctx.queue,
            // 每个 f32 采样 2 字节（两个字共 4 B）。
            u64::from(offset) * 2,
            bytemuck::cast_slice(words),
        )?;
        self.sample_offsets
            .insert(sample_id, SampleRef { offset, gain });
        // 4 个 1 GiB 片段 = 2G 采样，远低于 u32 上限；饱和加法只是防御。
        self.samples_next_offset = offset.saturating_add(len);
        Ok(grown)
    }

    /// Uploads up to `max_bytes` of the given samples (resampling first,
    /// smallest first so a small budget still completes several samples).
    /// Returns how many samples were uploaded; the rest stay pending for
    /// the next call. Used both by `upload_new_samples` (voice-driven) and
    /// `prefetch_samples` (lookahead-driven, realtime playback).
    pub(crate) fn upload_samples(
        &mut self,
        needed: &[usize],
        max_bytes: usize,
    ) -> Result<usize, SynthError> {
        let sf = match self.sf.as_mut() {
            Some(sf) => sf,
            None => return Ok(0),
        };
        if needed.is_empty() || max_bytes == 0 {
            return Ok(0);
        }

        let mut ids: Vec<(usize, usize)> = needed
            .iter()
            // REF-LGS-007：GPU 上每采样 2 B（2×snorm16 打包），预算按新口径算。
            .map(|&id| (id, sf.sample_data(id).len() * 2))
            .collect();
        ids.sort_by_key(|&(_, bytes)| bytes);
        let mut budget = max_bytes;
        let mut todo: Vec<usize> = Vec::new();
        for (id, bytes) in ids {
            if budget < bytes {
                continue; // does not fit the remaining budget; try next time
            }
            todo.push(id);
            budget -= bytes;
        }
        if todo.is_empty() && !needed.is_empty() {
            // No sample fits the budget (large samples): upload the smallest
            // one anyway so progress is never zero - a single sample cannot
            // be split across blocks.
            let Some(&smallest) = needed.iter().min_by_key(|&&id| sf.sample_data(id).len()) else {
                // `needed` 非空（上方已判定）；理论不可达，防御性兜底不 panic
                return Ok(0);
            };
            todo.push(smallest);
        }
        if todo.is_empty() {
            return Ok(0);
        }

        let rate = self.config.sample_rate;
        // Resampling is the dominant CPU cost for large soundfonts; run it
        // in parallel (each sample is independent), then upload sequentially.
        // REF-LGS-007：量化（打包）也在并行段完成——大音源串行打包会拖慢短渲染。
        let resampled: Vec<PreparedSample> = todo
            .par_iter()
            .map(|&sample_id| {
                let data = sf.resample_read(sample_id, rate);
                let (words, gain) = crate::gpu::pack_samples_2x16snorm(&data);
                (sample_id, data, words, gain)
            })
            .collect();

        for (sample_id, data, words, gain) in resampled {
            if let Some(sf) = self.sf.as_mut() {
                sf.cache_resampled(sample_id, rate, data.clone());
            }
            let len = data.len() as u32;
            if self.upload_sample_packed(sample_id, len, &words, gain)? {
                self.render_bg_dirty = true;
            }
        }
        Ok(todo.len())
    }

    /// Uploads every sample the current voices need (bounded only by the
    /// GPU buffer). Voice-driven path; see `upload_samples`.
    pub(crate) fn upload_new_samples(&mut self) -> Result<(), SynthError> {
        let mut needed: Vec<usize> = Vec::new();
        for v in &self.voices {
            if !self.sample_offsets.contains_key(&v.sample_id) {
                needed.push(v.sample_id);
            }
            if !self.sample_offsets.contains_key(&v.sample_id_r) {
                needed.push(v.sample_id_r);
            }
        }
        needed.sort_unstable();
        needed.dedup();
        self.upload_samples(&needed, usize::MAX)?;
        Ok(())
    }

    /// Realtime-playback lookahead: pre-uploads samples the event stream
    /// will use within the next ~2 seconds, in chunks of at most
    /// `max_bytes`, so the render thread never stalls on a multi-hundred-ms
    /// resample+upload inside a block (which empties the audio queue and
    /// crackles). Returns how many samples were uploaded this call.
    ///
    /// # Errors
    ///
    /// Returns [`SynthError::Gpu`] if a sample upload fails.
    pub fn prefetch_samples(&mut self, max_bytes: usize) -> Result<usize, SynthError> {
        let Some(sf) = self.sf.as_ref() else {
            return Ok(0);
        };
        let horizon = self.global_frame + self.config.sample_rate as u64 * 2;
        let mut needed: Vec<usize> = Vec::new();
        for ev in self.offline_events.iter().skip(self.offline_cursor) {
            if (ev.sample as u64) > horizon {
                break;
            }
            if let MidiEvent::NoteOn { key, vel } = ev.event() {
                for &zid in sf.zones_at(key, vel) {
                    let z = sf.zone(zid);
                    if !self.sample_offsets.contains_key(&z.sample_id) {
                        needed.push(z.sample_id);
                    }
                    if !self.sample_offsets.contains_key(&z.sample_id_r) {
                        needed.push(z.sample_id_r);
                    }
                }
            }
        }
        needed.sort_unstable();
        needed.dedup();
        self.upload_samples(&needed, max_bytes)
    }
}
