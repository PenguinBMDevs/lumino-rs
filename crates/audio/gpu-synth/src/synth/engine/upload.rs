use super::*;

impl GpuSynth {
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
            .map(|&id| (id, sf.sample_data(id).len() * 4))
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
            let smallest = needed
                .iter()
                .min_by_key(|&&id| sf.sample_data(id).len())
                .copied()
                .unwrap();
            todo.push(smallest);
        }
        if todo.is_empty() {
            return Ok(0);
        }

        let device = &self.res.ctx.device;
        let queue = &self.res.ctx.queue;
        let rate = self.config.sample_rate;
        // Resampling is the dominant CPU cost for large soundfonts; run it
        // in parallel (each sample is independent), then upload sequentially.
        let resampled: Vec<(usize, Arc<[f32]>)> = todo
            .par_iter()
            .map(|&sample_id| {
                let data = sf.resample_read(sample_id, rate);
                (sample_id, data)
            })
            .collect();

        for (sample_id, data) in resampled {
            sf.cache_resampled(sample_id, rate, data.clone());
            let len = data.len() as u32;
            let offset = self.samples_next_offset;
            let grown = write_samples(
                &mut self.samples_chunks,
                device,
                queue,
                offset as u64 * 4,
                bytemuck::cast_slice(&data),
            )?;
            if grown {
                self.render_bg_dirty = true;
            }
            self.sample_offsets.insert(sample_id, (offset, len));
            self.samples_next_offset = offset + len;
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
