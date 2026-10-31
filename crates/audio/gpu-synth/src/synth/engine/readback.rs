use super::*;

impl GpuSynth {
    /// Polls the pending submission (dispatched by the previous
    /// `render_block`) and reads its audio + voice states back into
    /// `last_out`/`last_states`. Called at the start of every
    /// `render_block`, including silent fast-path blocks: the GPU has had a
    /// full block of CPU work (apply/upload) to finish the pending
    /// submission, so the poll returns immediately instead of stalling.
    ///
    /// # Errors
    ///
    /// Returns [`SynthError::Gpu`] if the poll or either map fails.
    pub(crate) fn collect_pending_readback(&mut self) -> Result<(), SynthError> {
        let Some(p) = self.pending.take() else {
            return Ok(());
        };
        let device = &self.res.ctx.device;

        // Map requests (callbacks fire inside the poll below).
        let (otx, orx) = std::sync::mpsc::channel();
        self.out_readback[p.out_slot]
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| {
                let _ = otx.send(r.is_ok());
            });
        let (stx, srx) = std::sync::mpsc::channel();
        let states_rb = self.states_readback[p.states_slot].buffer().clone();
        states_rb
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| {
                let _ = stx.send(r.is_ok());
            });

        // REND-019 #152：**有界等待**——分片 poll（100ms）+ 本地丢失标志检查 +
        // 回调 try_recv；任一失败/超时立即以明确错误收场。旧实现
        // `Wait{timeout:None}` + 无超时 `recv()`：设备中途丢失（TDR）时存在
        // 永久挂死窗口（离线导出尤其致命）。
        // 正常路径：GPU 早已完成（一个 block 的 CPU 工作量），首个分片即返回。
        const WAIT_SLICE: std::time::Duration = std::time::Duration::from_millis(100);
        const WAIT_MAX: std::time::Duration = std::time::Duration::from_secs(10);
        let deadline = std::time::Instant::now() + WAIT_MAX;
        let mut out_res: Option<bool> = None;
        let mut states_res: Option<bool> = None;
        loop {
            if self.res.ctx.is_lost() {
                return Err(SynthError::Gpu("device lost during readback".into()));
            }
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: Some(p.idx.clone()),
                    timeout: Some(WAIT_SLICE),
                })
                .map_err(|e| SynthError::Gpu(format!("poll failed: {e:?}")))?;
            if out_res.is_none()
                && let Ok(r) = orx.try_recv()
            {
                out_res = Some(r);
            }
            if states_res.is_none()
                && let Ok(r) = srx.try_recv()
            {
                states_res = Some(r);
            }
            if out_res.is_some() && states_res.is_some() {
                break;
            }
            if std::time::Instant::now() >= deadline {
                return Err(SynthError::Gpu("readback timeout".into()));
            }
        }

        let out_ok = out_res.unwrap_or(false);
        if out_ok {
            let slice = self.out_readback[p.out_slot].slice(..);
            self.last_out = Some(slice.get_mapped_range().to_vec());
            self.out_readback[p.out_slot].unmap();
        } else {
            return Err(SynthError::Gpu("output readback map failed".into()));
        }
        let states_ok = states_res.unwrap_or(false);
        if states_ok {
            let rb = self.states_readback[p.states_slot].buffer();
            let mapped = rb.slice(..).get_mapped_range();
            let bytes = mapped.to_vec();
            drop(mapped);
            self.last_states = Some(bytes);
            rb.unmap();
        } else {
            return Err(SynthError::Gpu("states readback map failed".into()));
        }

        // Both readback buffers are free again; alternate the slots.
        self.out_readback_cur ^= 1;
        self.states_readback_cur ^= 1;
        // The poll confirmed the pending submission (and every earlier one)
        // finished, so every closed belt chunk is safe to reclaim. wgpu 27's
        // StagingBelt additionally guards reuse itself: recalled chunks are
        // re-mapped and only re-enter the free pool after the GPU releases
        // them, so a chunk can never be overwritten while in flight.
        self.belt.recall();
        Ok(())
    }

    pub(crate) fn readback(&mut self, out: &mut [f32]) -> Result<(), SynthError> {
        let Some(data) = self.last_out.take() else {
            // First block of a stream has no previous dispatch to collect
            // from: the pipeline owes the listener silence for that block
            // (block 0's audio arrives with block 1).
            out.fill(0.0);
            return Ok(());
        };
        let count = (data.len() / 4).min(out.len());
        out[..count].copy_from_slice(bytemuck::cast_slice(&data[..count * 4]));
        // Lookahead output limiter (skip with LUMINO_NO_LIMITER for GPU
        // output diagnostics).
        if std::env::var("LUMINO_NO_LIMITER").is_err() {
            self.apply_limiter(out);
        }
        Ok(())
    }

    /// Lookahead output peak limiter (see the `limiter_gain` field doc).
    ///
    /// The mix pass returns the raw voice sum (f32 holds it losslessly even
    /// at hundreds/thousands of voices). A limiter is needed because the sum
    /// routinely exceeds full scale at high polyphony (64 voices peak ~10x,
    /// 800+ voices hundreds of times), and a hard clip / per-sample soft clip
    /// flat-tops the waveform into square-wave distortion (verified
    /// empirically, see the module history).
    ///
    /// WHY LOOKAHEAD (learned the hard way from the previous block-peak
    /// limiter): a limiter whose gain follows the signal with a finite
    /// response time faces a contradiction. A FAST gain (0.5 ms attack)
    /// tracks the signal tightly but the gain change itself is a fast
    /// multiplicative modulation of the whole mix - at 800+ live voices,
    /// where the sum exceeds full scale almost constantly and the block peak
    /// swings with every note, this modulation is a CONTINUOUS CRACKLE
    /// (the "800-1000 voices and above it starts popping" report). A SLOW
    /// gain (the other escape) avoids the modulation but lets onsets
    /// overshoot far past full scale before the gain descends - which then
    /// needs a deep soft clip that flat-tops the waveform again.
    ///
    /// The lookahead design resolves the contradiction: the mix block is
    /// already fully computed in RAM when the limiter runs, so the gain at
    /// frame `i` can be derived from the PEAK OF THE NEXT 64 FRAMES (1 ms @
    /// 64 kHz) instead of the past. The gain is therefore fully settled
    /// BEFORE the loud samples arrive: no overshoot, no fast modulation
    /// during onsets. The output is the mix delayed by 1 ms (a fixed,
    /// inaudible latency - 1 ms is far below the ~10 ms humans perceive).
    /// The gain itself still moves slowly (0.5 ms attack / 80 ms release),
    /// so it never modulates the signal audibly.
    ///
    /// The block's last 64 frames cannot see the next block, so their
    /// lookahead window is truncated; if the next block opens louder than
    /// this block's tail, a brief (< 1 ms) overshoot passes through
    /// `soft_knee` (a slope-continuous ceiling, not a flat-top clip).
    ///
    /// Below the 0.98 threshold (gain at unity) the limiter reduces to a
    /// pure 1 ms delay - the waveform is preserved bit-exactly, just shifted.
    pub(crate) fn apply_limiter(&mut self, out: &mut [f32]) {
        let sr = self.config.sample_rate as f32;
        // REF-LGS-008（A2）：块 ≥ 512 帧时按绝对帧对齐的 512 帧窗口处理，
        // 增益轨迹因此与渲染块长无关（block 512 的序列是唯一权威）。
        let window = self.config.block_size.min(LIMITER_WINDOW);
        limit_stream(
            out,
            &mut self.limiter_tail,
            &mut self.limiter_gain,
            sr,
            window,
        );
    }

    /// Applies the read-back voice states to the CPU mirror.
    ///
    /// Runs right after `collect_pending_readback` and BEFORE
    /// `upload_voices`, so the mirror reflects the GPU state of the block
    /// just read back (the one the next dispatch resumes from). A voice
    /// that ended on the GPU gets `v.state.ended = 1` here, which lets
    /// `upload_voices` prune it this same block.
    ///
    /// Maps by voice id (`prev_voice_ids` records the last upload order):
    /// the list may have shrunk since, so positional lookup would apply a
    /// stale state (and miss `ended`) on the wrong voice.
    ///
    /// Does NOT consume `prev_voice_ids`: that list must stay aligned with
    /// `last_states` (same upload order) for `upload_voices`' resume
    /// matching, which runs on the very same read-back states.
    pub(crate) fn sync_voice_states(&mut self) {
        let Some(states) = self.last_states.as_ref() else {
            return;
        };
        let count = states.len() / VoiceState::SIZE;
        if count == 0 {
            return;
        }
        // `prev_voice_ids` and `self.voices` are both sorted by voice id
        // (monotonic counter, `retain` keeps order), so a two-pointer merge
        // maps states back onto the mirror in O(n) - no HashMap build.
        let ids = &self.prev_voice_ids;
        let mut k = 0usize;
        for v in self.voices.iter_mut() {
            while k < ids.len() && ids[k] < v.id {
                k += 1;
            }
            if k >= ids.len() || ids[k] != v.id || k >= count {
                continue;
            }
            let off = k * VoiceState::SIZE;
            let st: &VoiceState = bytemuck::from_bytes(&states[off..off + VoiceState::SIZE]);
            v.state = *st;
            if st.ended != 0 {
                v.released = true;
            }
        }
    }
}
