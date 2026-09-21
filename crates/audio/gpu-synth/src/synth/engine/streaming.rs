use super::*;

impl GpuSynth {
    pub(crate) fn apply_events_streaming(
        &mut self,
        stream: &mut MidiStream,
        end: u64,
    ) -> Result<(), SynthError> {
        while let Some(ev) = self.pending_events.pop_front() {
            self.handle_event(ev)?;
        }
        while let Some(ev) = stream.peek() {
            if ev.sample as u64 >= end {
                break;
            }
            let ev = stream.next_event().expect("peeked event must exist");
            self.handle_event(ev)?;
        }
        Ok(())
    }

    pub(crate) fn render_block_streaming(
        &mut self,
        out: &mut [f32],
        stream: &mut MidiStream,
    ) -> Result<(), SynthError> {
        let block = self.config.block_size;
        let chs = self.output_channels();
        if out.len() < block * chs {
            return Err(SynthError::Config("output buffer too small".into()));
        }
        let base = self.global_frame;
        self.apply_events_streaming(stream, base + block as u64)?;
        self.collect_pending_readback()?;
        if self.voices.is_empty() {
            self.update_mix_params(base)?;
            if let Some(data) = self.last_out.take() {
                let count = (data.len() / 4).min(block * chs);
                out[..count].copy_from_slice(bytemuck::cast_slice(&data[..count * 4]));
                if count < block * chs {
                    out[count..block * chs].fill(0.0);
                }
            } else {
                out[..block * chs].fill(0.0);
            }
            self.apply_limiter(&mut out[..block * chs]);
            self.global_frame += block as u64;
            return Ok(());
        }
        self.sync_voice_states();
        self.upload_voices(base)?;
        self.upload_new_samples()?;
        self.update_mix_params(base)?;
        self.dispatch(base)?;
        self.readback(out)?;
        self.global_frame += block as u64;
        Ok(())
    }

    /// Streaming offline render that writes directly to `wav_path` without
    /// ever holding the MIDI event array or the full sample buffer in memory.
    ///
    /// The MIDI file is consumed via [`MidiStream`] (heap-merged, 8 bytes
    /// saved per event vs `MidiFile`) and audio is flushed block-by-block
    /// through [`crate::audio::wav::WavStreamWriter`]. Peak memory is
    /// therefore `O(tracks + block)` instead of `O(events + samples)`.
    pub fn render_midi_to_wav_streaming(
        &mut self,
        midi_path: impl AsRef<std::path::Path>,
        wav_path: impl AsRef<std::path::Path>,
        limit_frames: Option<u64>,
    ) -> Result<RenderResult, SynthError> {
        // Reset state exactly like `render_midi_inner`
        self.offline_cursor = 0;
        self.offline_events = Vec::new();
        self.voices.clear();
        for q in self.key_voices.iter_mut() {
            q.clear();
        }
        self.spawn_budget = [0; 16 * 128];
        self.active_notes = [0; 16 * 128];
        self.global_frame = 0;
        self.active_voice_count = 0;
        self.last_states = None;
        self.last_out = None;
        self.prev_voice_ids.clear();
        self.pending = None;
        self.pending_events.clear();
        self.pending_mix_events.clear();

        let prof = std::env::var("LUMINO_PROFILE").is_ok();
        let t0 = std::time::Instant::now();
        let mut stream = MidiStream::open(midi_path.as_ref(), self.config.sample_rate)?;
        let t1 = std::time::Instant::now();
        // Pre-warm: collect wanted samples via raw track scan (O(n), no heap)
        // — same set as `render_midi_inner` but without consuming the stream,
        // so no `rewind` needed. The old heap-scan produced 299 sample diffs
        // when skipped (lazy per-block uploads race the pipeline).
        //
        // 进度回调先克隆出来：下面的上传循环持有 `&mut self.sf`，不能借用 `self`。
        let progress_cb = self.render_progress.clone();
        let mut prewarm_total: u64 = 0;
        let mut prewarm_done: u64 = 0;
        {
            let mut wanted: Vec<usize> = Vec::new();
            if let Some(sf_ref) = self.sf.as_ref() {
                stream.for_each_note_on(|key, vel| {
                    for &zid in sf_ref.zones_at(key, vel) {
                        let z = sf_ref.zone(zid);
                        wanted.push(z.sample_id);
                        wanted.push(z.sample_id_r);
                    }
                });
                wanted.sort_unstable();
                wanted.dedup();
            }
            if !wanted.is_empty() {
                prewarm_total = wanted.len() as u64;
                let rate = self.config.sample_rate;
                // Chunked resample+upload to keep peak <100 MB (was holding all Arcs at once: 200 MB+)
                let mut grown = false;
                for chunk in wanted.chunks(16) {
                    self.check_render_checkpoint()?;
                    let pre: Vec<(usize, Arc<[f32]>)> = if let Some(sf_ref) = self.sf.as_ref() {
                        chunk
                            .par_iter()
                            .map(|&id| (id, sf_ref.resample_uncached(id, rate)))
                            .collect()
                    } else {
                        Vec::new()
                    };
                    let Some(sf_mut) = self.sf.as_mut() else {
                        continue;
                    };
                    let device = &self.res.ctx.device;
                    let queue = &self.res.ctx.queue;
                    for (id, data) in pre {
                        sf_mut.cache_resampled(id, rate, data.clone());
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
                    prewarm_done += chunk.len() as u64;
                    report_progress(
                        &progress_cb,
                        RenderProgress::Prewarm {
                            done: prewarm_done,
                            total: prewarm_total,
                        },
                    );
                }
                if grown {
                    self.render_bg_dirty = true;
                }
            }
        }
        if prewarm_total > 0 {
            report_progress(
                &progress_cb,
                RenderProgress::Prewarm {
                    done: prewarm_total,
                    total: prewarm_total,
                },
            );
        }

        let events_end = stream.end_sample();
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
        let mut writer = crate::audio::wav::WavStreamWriter::create(
            wav_path.as_ref(),
            self.config.sample_rate,
            chs as u16,
        )?;
        let mut block_buf = vec![0.0f32; block * chs];
        let mut progress = ProgressBar::new(max_frames, self.config.show_progress);
        let mut first_block = true;

        // Phase 1: events + decay interleaved, streaming block by block
        loop {
            let events_done = stream.is_exhausted();
            if events_done && self.voices.is_empty() {
                if prof {
                    eprintln!(
                        "[render-stream] break: events_done+empty at frame {}",
                        self.global_frame
                    );
                }
                break;
            }
            self.check_render_checkpoint()?;
            let rb_t0 = std::time::Instant::now();
            self.render_block_streaming(&mut block_buf, &mut stream)?;
            let rb_dt = rb_t0.elapsed();
            progress.tick(self.global_frame);
            self.report_render_progress(RenderProgress::Render {
                done: self.global_frame.min(max_frames),
                total: max_frames,
            });
            self.check_memory()?;
            let silent = block_buf.iter().all(|s| s.abs() <= threshold);
            if rb_dt.as_millis() > 30 {
                eprintln!(
                    "[slow-block-stream] frame={} render={:?} silent={} voices={}",
                    self.global_frame,
                    rb_dt,
                    silent,
                    self.voices.len()
                );
            }
            if events_done && silent {
                if prof {
                    eprintln!(
                        "[render-stream] break: events_done+silent at frame {}",
                        self.global_frame
                    );
                }
                break;
            }
            if !first_block {
                writer.write_samples(&block_buf)?;
            }
            first_block = false;
            if self.global_frame >= max_frames {
                if limited {
                    break;
                }
                return Err(self.render_timeout(&block_buf));
            }
        }

        // Phase 2: tail
        loop {
            self.check_render_checkpoint()?;
            self.render_block_streaming(&mut block_buf, &mut stream)?;
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
                if (self.global_frame - block as u64) < max_frames {
                    writer.write_samples(&block_buf)?;
                }
                break;
            }
            writer.write_samples(&block_buf)?;
        }

        // Drain pipeline
        self.render_block_streaming(&mut block_buf, &mut stream)?;
        if block_buf.iter().any(|s| s.abs() > threshold) {
            writer.write_samples(&block_buf)?;
        }
        let frames = writer.frames_written();
        writer.finalize()?;
        progress.finish();

        if prof {
            let t2 = std::time::Instant::now();
            eprintln!(
                "[profile-stream] midi load: {:?}, render loops: {:?}, flush: {:?}",
                t1 - t0,
                t2 - t1,
                t2.elapsed()
            );
        }

        Ok(RenderResult {
            samples: Vec::new(),
            sample_rate: self.config.sample_rate,
            channels: chs as u32,
            frames,
        })
    }

    /// Convenience: streaming render of a whole file to `wav_path`.
    pub fn render_midi_file_to_wav_streaming(
        &mut self,
        midi_path: impl AsRef<std::path::Path>,
        wav_path: impl AsRef<std::path::Path>,
    ) -> Result<RenderResult, SynthError> {
        self.render_midi_to_wav_streaming(midi_path, wav_path, None)
    }

    /// Convenience: streaming render of the first `frames` frames to `wav_path`.
    pub fn render_midi_frames_to_wav_streaming(
        &mut self,
        midi_path: impl AsRef<std::path::Path>,
        wav_path: impl AsRef<std::path::Path>,
        frames: u64,
    ) -> Result<RenderResult, SynthError> {
        self.render_midi_to_wav_streaming(midi_path, wav_path, Some(frames))
    }

    // ------------------------------------------------------------------
    // Internals
    // ------------------------------------------------------------------

    pub(crate) fn output_channels(&self) -> usize {
        if self.config.channels == ChannelMode::Stereo {
            2
        } else {
            1
        }
    }
}
