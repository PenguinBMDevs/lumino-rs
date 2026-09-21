use super::*;

impl GpuSynth {
    /// Renders one block of `block_size` frames into `out` (interleaved
    /// L/R, length `block_size * channels`).
    ///
    /// # Errors
    ///
    /// Returns [`SynthError::Gpu`] on dispatch/readback failures.
    pub fn render_block(&mut self, out: &mut [f32]) -> Result<(), SynthError> {
        let block = self.config.block_size;
        let chs = self.output_channels();
        if out.len() < block * chs {
            return Err(SynthError::Config("output buffer too small".into()));
        }

        let prof = std::env::var("LUMINO_PROFILE").is_ok();
        let base = self.global_frame;

        let t0 = std::time::Instant::now();
        self.apply_events(base, base + block as u64)?;
        let t1 = std::time::Instant::now();

        // One-block pipeline: consume the previous block's readback BEFORE
        // the fast-path check, because a silent block still owes the
        // listener the previous block's audio (the GPU rendered it while we
        // prepared this block). The pending submission has had a full block
        // of CPU work to finish, so the poll returns immediately.
        self.collect_pending_readback()?;
        let t1b = std::time::Instant::now();

        // Fast path: no voices at all - the block is pure silence (the mix
        // pass would sum nothing). Advance the controller states so CC
        // smoothing stays continuous and skip the GPU round trip. Dense-
        // but-sparse MIDI spends a large fraction of its timeline in note
        // gaps, so this is a significant win.
        //
        // We did NOT dispatch this block, so the GPU state did not advance
        // and this block's own audio is silence; the output owed to the
        // listener is the audio of the last DISPATCHED block, collected
        // above - exactly one block old, which is this block's turn to play.
        // With nothing pending (two consecutive silent blocks) it is
        // silence. `last_states`/`prev_voice_ids` are left intact: they are
        // the resume state of the last dispatch, still valid because the GPU
        // state did not move.
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
            // The replayed block is the last DISPATCHED block's audio, which
            // the limiter would have scaled at its own readback; scale it
            // again so the limiter state stays continuous either way.
            self.apply_limiter(&mut out[..block * chs]);
            self.global_frame += block as u64;
            if prof {
                eprintln!(
                    "[profile] block {}: silent skip",
                    self.global_frame / block as u64 - 1
                );
            }
            return Ok(());
        }

        // States from the same readback: apply the previous block's GPU
        // state to the CPU mirror BEFORE uploading this block's parameters,
        // so `upload_voices` (a) prunes voices that ended on the GPU and
        // (b) has a fresh `v.state` fallback when a voice is not present in
        // the read-back list. `sync_voice_states` does not consume
        // `prev_voice_ids` - that list must stay aligned with `last_states`
        // for `upload_voices`' resume matching.
        self.sync_voice_states();
        let t1c = std::time::Instant::now();

        self.upload_voices(base)?;
        let t2 = std::time::Instant::now();
        self.upload_new_samples()?;
        let t3 = std::time::Instant::now();
        self.update_mix_params(base)?;
        let t4 = std::time::Instant::now();
        self.dispatch(base)?;
        let t5 = std::time::Instant::now();
        // The output owed this block is the audio collected at its start
        // (the previous block's render).
        self.readback(out)?;
        let t6 = std::time::Instant::now();

        if prof && self.global_frame.is_multiple_of(block as u64 * 25) {
            let block_no = self.global_frame / block as u64;
            eprintln!(
                "[profile] block {block_no}: apply={}us collect={}us sync={}us upload={}us samples={}us mix={}us dispatch={}us readback={}us total={}us voices={}",
                (t1 - t0).as_micros(),
                (t1b - t1).as_micros(),
                (t1c - t1b).as_micros(),
                (t2 - t1c).as_micros(),
                (t3 - t2).as_micros(),
                (t4 - t3).as_micros(),
                (t5 - t4).as_micros(),
                (t6 - t5).as_micros(),
                (t6 - t0).as_micros(),
                self.voices.len()
            );
        }

        self.global_frame += block as u64;
        Ok(())
    }

    /// Renders a full MIDI file to memory, stopping once all voices have
    /// decayed below the silence threshold (mirroring XSynth's offline
    /// renderer).
    ///
    /// # Errors
    ///
    /// Returns [`SynthError::Midi`] if the file cannot be parsed, or
    /// [`SynthError::Gpu`] on GPU failures.
    pub fn render_midi_file(
        &mut self,
        midi_path: impl AsRef<std::path::Path>,
    ) -> Result<RenderResult, SynthError> {
        self.render_midi_inner(midi_path, None)
    }

    /// Renders the first `frames` frames of a MIDI file (used to compare the
    /// beginning of long MIDIs without rendering the whole piece).
    ///
    /// # Errors
    ///
    /// Returns [`SynthError::Midi`] if the file cannot be parsed, or
    /// [`SynthError::Gpu`] on GPU failures.
    pub fn render_midi_frames(
        &mut self,
        midi_path: impl AsRef<std::path::Path>,
        frames: u64,
    ) -> Result<RenderResult, SynthError> {
        self.render_midi_inner(midi_path, Some(frames))
    }

    /// Builds the error reported when offline rendering exceeds its frame
    /// budget (a voice never finished).
    pub(crate) fn render_timeout(&self, last_block: &[f32]) -> SynthError {
        let last_peak = last_block.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        SynthError::RenderTimeout {
            frames: self.global_frame,
            active_voices: self.voices.len(),
            last_peak,
        }
    }

    const MAX_CPU_MEM_BYTES: u64 = 100 * 1024 * 1024;

    pub(crate) fn check_memory(&self) -> Result<(), SynthError> {
        // Heuristic MIDI budget — file + Vec<TimedEvent> must stay <100 MB.
        // With true file streaming the heap is O(tracks + block); voices dominate.
        let voices_mem =
            (self.voices.len() * std::mem::size_of::<crate::synth::voices::Voice>()) as u64;
        let upload_mem = (self.upload_params.len() * std::mem::size_of::<crate::gpu::VoiceParams>()
            + self.upload_states.len() * std::mem::size_of::<crate::gpu::VoiceState>())
            as u64;
        let total = voices_mem + upload_mem + 8 * 1024 * 4; // 4 tracks × 8 KiB
        if total > Self::MAX_CPU_MEM_BYTES {
            return Err(SynthError::Config(format!(
                "MIDI/CPU budget {} bytes exceeds 100 MB (voices {} upload {} KiB)",
                total,
                self.voices.len(),
                upload_mem / 1024
            )));
        }
        if self
            .global_frame
            .is_multiple_of(self.config.block_size as u64 * 25)
        {
            eprintln!(
                "[mem] midi≈{} MB voices={} upload≈{} KiB",
                total / (1024 * 1024),
                self.voices.len(),
                upload_mem / 1024
            );
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Streaming offline render — zero `Vec<TimedEvent>` / zero full-sample Vec
    // ------------------------------------------------------------------
}
