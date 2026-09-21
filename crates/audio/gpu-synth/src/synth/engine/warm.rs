use super::*;

impl GpuSynth {
    /// Forces one full GPU render pass (upload + dispatch + readback) so the
    /// driver compiles the pipelines and the first *real* block does not pay
    /// a hundreds-of-milliseconds cold-start stall (which empties the audio
    /// queue and causes crackle on dense MIDI).
    ///
    /// Safe to call before any notes are played; it renders a silent block.
    ///
    /// # Errors
    ///
    /// Returns [`SynthError::Gpu`] on GPU failures.
    pub fn warm_gpu(&mut self) -> Result<(), SynthError> {
        // A single voice with amp 0 renders silently but exercises the full
        // pipeline (upload, dispatch, readback). Save/restore the global
        // frame so the warm-up does not advance the timeline.
        if self.sf.is_none() {
            return Ok(());
        }
        let saved_frame = self.global_frame;
        let saved_voices = std::mem::take(&mut self.voices);
        for q in self.key_voices.iter_mut() {
            q.clear();
        }

        // Borrow sf to build one minimal voice.
        let mut buf = vec![0.0f32; self.config.block_size * self.output_channels()];
        if let Some(sf) = self.sf.as_ref()
            && let Some(&zid) = sf.zones_at(60, 100).first()
            && let Some(mut v) = build_voice(
                sf,
                zid,
                60,
                100,
                0,
                0,
                self.config.sample_rate,
                1.0,
                None,
                None,
                self.config.envelope_curves,
            )
        {
            v.amp = 0.0; // silent
            v.id = self.voice_id_counter;
            self.voice_id_counter += 1;
            self.voices.push(v);
        }
        if self.voices.is_empty() {
            // No soundfont/zone; nothing to warm. Restore and return.
            self.global_frame = saved_frame;
            return Ok(());
        }

        self.upload_voices(0)?;
        self.upload_new_samples()?;
        self.update_mix_params(0)?;
        self.dispatch(0)?;
        let _ = self.readback(&mut buf);

        // Restore state: drop the warm-up voice and reset the timeline.
        self.voices = saved_voices;
        for q in self.key_voices.iter_mut() {
            q.clear();
        }
        for (i, v) in self.voices.iter().enumerate() {
            self.key_voices[v.channel as usize * 128 + v.key as usize].push_back(i);
        }
        self.global_frame = saved_frame;
        self.active_voice_count = 0;
        self.last_out = None;
        self.last_states = None;
        self.prev_voice_ids.clear();
        Ok(())
    }

    /// Pre-builds the voice template cache for every (key, vel, channel)
    /// the MIDI uses, so the first dense blocks of realtime playback do not
    /// pay the per-note zone lookup + envelope build cost (which can spike
    /// the render load of the opening blocks of black-MIDI).
    pub(crate) fn warm_voice_templates(&mut self, events: &[TimedEvent]) {
        let Some(sf) = self.sf.as_ref() else {
            return;
        };
        let rate = self.config.sample_rate;
        let curves = self.config.envelope_curves;
        // Bitmap over the (channel x key x vel) grid: the HashMap lookup per
        // event cost ~50ns x 200M events on black-MIDI; this is O(1) array
        // indexing. Templates are built for the DEFAULT channel state only
        // (pitch 1.0, no env CC) - notes with bends or CC72/73 fall back to
        // building on demand.
        let mut seen: Vec<u8> = vec![0; 16 * 128 * 128];
        for ev in events {
            let MidiEvent::NoteOn { key, vel } = ev.event() else {
                continue;
            };
            let slot =
                &mut seen[ev.channel() as usize * 128 * 128 + key as usize * 128 + vel as usize];
            if *slot != 0 {
                continue;
            }
            *slot = 1;
            let tmpl_key = (key, vel, ev.channel(), 1.0f32.to_bits(), 0xFF, 0xFF);
            if self.voice_templates.contains_key(&tmpl_key) {
                continue;
            }
            let mut built: Vec<Voice> = Vec::new();
            for &zid in sf.zones_at(key, vel) {
                if let Some(v) = build_voice(
                    sf,
                    zid,
                    key,
                    vel,
                    ev.channel(),
                    0,
                    rate,
                    1.0,
                    None,
                    None,
                    curves,
                ) {
                    built.push(v);
                }
            }
            if !built.is_empty() {
                self.voice_templates.insert(tmpl_key, built);
            }
        }
    }

    /// Pre-warms the GPU sample cache with every sample the MIDI file will
    /// use (resampled and uploaded up front).
    ///
    /// Use this before realtime playback so the render loop never stalls on
    /// a lazily-resampled sample during dense sections — otherwise a single
    /// large sample can take hundreds of milliseconds to resample+upload in
    /// the middle of a block, emptying the audio queue and causing crackle.
    ///
    /// # Errors
    ///
    /// Returns [`SynthError::Midi`] if the file cannot be parsed, or
    /// [`SynthError::Gpu`] on GPU failures.
    pub fn prewarm_midi_file(
        &mut self,
        midi_path: impl AsRef<std::path::Path>,
    ) -> Result<(), SynthError> {
        let t0 = std::time::Instant::now();
        let midi = MidiFile::load(midi_path, self.config.sample_rate)?;
        let events = &midi.sequence.events;
        if let Some(sf) = self.sf.as_ref() {
            // One pass over the (possibly 100M+ event) stream. Black-MIDI
            // repeats the same (key, vel) millions of times, so a bitmap of
            // the 128x128 key/velocity grid skips `zones_at` for repeats -
            // the previous version called `zones_at` per event (100M+ calls)
            // and scanned the stream TWICE (samples + templates), which is
            // why prewarming "Rekt Apple!!.mid" took 90+ seconds.
            let mut seen: Vec<u8> = vec![0; 128 * 128];
            let mut wanted: Vec<usize> = Vec::new();
            for ev in events {
                let MidiEvent::NoteOn { key, vel } = ev.event() else {
                    continue;
                };
                let slot = &mut seen[key as usize * 128 + vel as usize];
                if *slot != 0 {
                    continue;
                }
                *slot = 1;
                for &zid in sf.zones_at(key, vel) {
                    let zone = sf.zone(zid);
                    wanted.push(zone.sample_id);
                    wanted.push(zone.sample_id_r);
                }
            }
            wanted.sort_unstable();
            wanted.dedup();
            let rate = self.config.sample_rate;
            let pre: Vec<(usize, Arc<[f32]>)> = wanted
                .par_iter()
                .map(|&id| (id, sf.resample_uncached(id, rate)))
                .collect();
            let sf = self.sf.as_mut().expect("soundfont present");
            let device = &self.res.ctx.device;
            let queue = &self.res.ctx.queue;
            let mut grown = false;
            for (id, data) in pre {
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
        // Warm the GPU pipelines so the first realtime block does not pay a
        // multi-hundred-ms cold start (which would empty the audio queue).
        self.warm_gpu()?;
        // Pre-build voice templates so the opening blocks do not pay the
        // per-note build cost either.
        self.warm_voice_templates(events);
        let t1 = std::time::Instant::now();
        if std::env::var("LUMINO_PROFILE").is_ok() {
            eprintln!(
                "[prewarm] load+scan+upload: {:.1}s, templates: {:.1}s",
                (t1 - t0).as_secs_f64(),
                t1.elapsed().as_secs_f64()
            );
        }
        Ok(())
    }
}
