use super::*;

impl GpuSynth {
    pub(crate) fn render_midi_inner(
        &mut self,
        midi_path: impl AsRef<std::path::Path>,
        limit_frames: Option<u64>,
    ) -> Result<RenderResult, SynthError> {
        self.offline_cursor = 0;
        self.offline_events = Vec::new();
        self.voices.clear();
        self.global_frame = 0;
        self.active_voice_count = 0;
        self.last_states = None;
        self.last_out = None;
        self.prev_voice_ids.clear();
        self.pending = None;

        let prof = std::env::var("LUMINO_PROFILE").is_ok();
        let t0 = std::time::Instant::now();
        let midi = MidiFile::load(midi_path, self.config.sample_rate)?;
        let t1 = std::time::Instant::now();
        self.offline_events = midi.sequence.events;

        // Pre-warm resampling AND upload: resolve every sample the MIDI will
        // use, resample it in parallel and upload it to the GPU up front, so
        // the render loop never stalls on a lazily-resampled sample or pays
        // per-block sample uploads during the dense sections.
        let mut prewarm_total: u64 = 0;
        if let Some(sf) = self.sf.as_ref() {
            let mut wanted: Vec<usize> = Vec::new();
            for ev in &self.offline_events {
                if let MidiEvent::NoteOn { key, vel } = ev.event() {
                    for &zid in sf.zones_at(key, vel) {
                        let zone = sf.zone(zid);
                        wanted.push(zone.sample_id);
                        wanted.push(zone.sample_id_r);
                    }
                }
            }
            wanted.sort_unstable();
            wanted.dedup();
            let rate = self.config.sample_rate;
            prewarm_total = wanted.len() as u64;
            let mut prewarm_done: u64 = 0;
            let mut pre: Vec<(usize, Arc<[f32]>)> = Vec::with_capacity(wanted.len());
            for chunk in wanted.chunks(64) {
                self.check_render_checkpoint()?;
                pre.par_extend(
                    chunk
                        .par_iter()
                        .map(|&id| (id, sf.resample_uncached(id, rate))),
                );
                prewarm_done += chunk.len() as u64;
                self.report_render_progress(RenderProgress::Prewarm {
                    done: prewarm_done,
                    total: prewarm_total,
                });
            }
            // The upload loop holds `&mut self.sf`, so the checkpoint handle is
            // cloned out and polled via the free function instead of `&self`.
            let checkpoint = self.render_checkpoint.clone();
            let sf = self.sf.as_mut().expect("soundfont present");
            let device = &self.res.ctx.device;
            let queue = &self.res.ctx.queue;
            let mut grown = false;
            for (id, data) in pre {
                checkpoint_ok(&checkpoint)?;
                sf.cache_resampled(id, rate, data.clone());
                let len = data.len() as u32;
                let offset = self.samples_next_offset;
                grown |= write_samples(
                    &mut self.samples_chunks,
                    device,
                    queue,
                    offset as u64 * 4,
                    bytemuck::cast_slice(&data),
                )?;
                self.sample_offsets.insert(id, (offset, len));
                self.samples_next_offset = offset + len;
            }
            if grown {
                self.render_bg_dirty = true;
            }
        }
        if prewarm_total > 0 {
            // 上传循环结束后统一收尾，保证导出侧看到 100% 预载。
            self.report_render_progress(RenderProgress::Prewarm {
                done: prewarm_total,
                total: prewarm_total,
            });
        }

        // Render timeout guard: the offline loops must terminate on their own
        // (events consumed + silence / no voices). A voice that can never
        // finish - held damper, missing note-off, pathological envelope -
        // would otherwise loop forever. Abort once the last event is behind
        // us by `max_tail_seconds`; the hard cap keeps the guard well inside
        // the u32 frame range used by the GPU parameters.
        let events_end = self.offline_events.last().map_or(0, |e| e.sample as u64);
        let tail_budget =
            (self.config.max_tail_seconds as f64 * self.config.sample_rate as f64) as u64;
        let max_frames = match limit_frames {
            Some(n) => n.min(MAX_RENDER_FRAMES),
            None => events_end
                .saturating_add(tail_budget)
                .min(MAX_RENDER_FRAMES),
        };
        let limited = limit_frames.is_some();

        let block = self.config.block_size;
        let chs = self.output_channels();
        let threshold = self.config.render_silence_threshold;
        let mut samples: Vec<f32> = Vec::new();
        let mut block_buf = vec![0.0f32; block * chs];

        // Progress reporting: the total is the render horizon (`max_frames`).
        // Phase 1 walks the event stream; the tail phase renders past the
        // last event, so the bar is allowed to exceed 100% there.
        let mut progress = ProgressBar::new(max_frames, self.config.show_progress);

        // The one-block pipeline makes every `render_block` output the audio
        // of the PREVIOUS dispatched block. The very first block therefore
        // emits the fake "-1" audio (silence, nothing was dispatched before
        // it) which must not enter the sample stream; block 0's real audio
        // arrives with block 1.
        let mut first_block = true;

        // Phase 1: process all events and render until no voices remain. If
        // the events are exhausted and the block went silent, we stop even
        // when voices linger (they are stuck in sustain and contribute
        // nothing; the tail below would be silent too).
        //
        // NOTE on the appending order: with the one-block pipeline every
        // `render_block` outputs the audio of the block it dispatched
        // PREVIOUSLY, so the frame range of `block_buf` lags `global_frame`
        // by one block. Appending must therefore happen BEFORE the
        // `max_frames` check - the block being appended is the one that
        // just crossed (or reached) the limit, i.e. the last in-limit
        // audio. Checking first would drop it and let the drain append an
        // out-of-limit block instead, shifting the whole tail by one block.
        loop {
            let events_done = self.offline_cursor >= self.offline_events.len();
            if events_done && self.voices.is_empty() {
                if prof {
                    eprintln!(
                        "[render] break: events_done+empty at frame {}",
                        self.global_frame
                    );
                }
                break;
            }
            self.check_render_checkpoint()?;
            let rb_t0 = std::time::Instant::now();
            self.render_block(&mut block_buf)?;
            let rb_dt = rb_t0.elapsed();
            progress.tick(self.global_frame);
            self.report_render_progress(RenderProgress::Render {
                done: self.global_frame.min(max_frames),
                total: max_frames,
            });
            let silent = block_buf.iter().all(|s| s.abs() <= threshold);
            if rb_dt.as_millis() > 30 {
                eprintln!(
                    "[slow-block] frame={} render={:?} silent={} cursor={} voices={}",
                    self.global_frame,
                    rb_dt,
                    silent,
                    self.offline_cursor,
                    self.voices.len()
                );
            }
            if events_done && silent {
                if prof {
                    eprintln!(
                        "[render] break: events_done+silent at frame {} cursor={}/{}",
                        self.global_frame,
                        self.offline_cursor,
                        self.offline_events.len()
                    );
                }
                break;
            }
            if !first_block {
                samples.extend_from_slice(&block_buf);
            }
            first_block = false;
            if self.global_frame >= max_frames {
                if limited {
                    break;
                }
                return Err(self.render_timeout(&block_buf));
            }
        }

        // Phase 2: decay tail - render blocks until one is entirely silent.
        loop {
            self.check_render_checkpoint()?;
            self.render_block(&mut block_buf)?;
            progress.tick(self.global_frame);
            self.report_render_progress(RenderProgress::Render {
                done: self.global_frame.min(max_frames),
                total: max_frames,
            });
            let silent = block_buf.iter().all(|s| s.abs() <= threshold);
            if silent {
                break;
            }
            if self.global_frame >= max_frames {
                if !limited {
                    return Err(self.render_timeout(&block_buf));
                }
                // Frame-limited: this output is the audio of the block that
                // crossed the limit (one block behind `global_frame`);
                // append it only if it still starts inside the limit, then
                // stop - rendering any further would only produce
                // out-of-limit audio.
                if (self.global_frame - block as u64) < max_frames {
                    samples.extend_from_slice(&block_buf);
                }
                break;
            }
            samples.extend_from_slice(&block_buf);
        }

        // Drain the pipeline: the data of the last submitted block is only
        // read back by one more render. Append it if it is not silence
        // (the loops above already consumed every non-silent block).
        self.render_block(&mut block_buf)?;
        if block_buf.iter().any(|s| s.abs() > threshold) {
            samples.extend_from_slice(&block_buf);
        }
        progress.finish();

        if prof {
            let t2 = std::time::Instant::now();
            eprintln!(
                "[profile] midi load: {:?}, render loops: {:?}, flush: {:?}",
                t1 - t0,
                t2 - t1,
                t2.elapsed()
            );
        }

        let frames = (samples.len() / chs) as u64;
        Ok(RenderResult {
            samples,
            sample_rate: self.config.sample_rate,
            channels: chs as u32,
            frames,
        })
    }
}
