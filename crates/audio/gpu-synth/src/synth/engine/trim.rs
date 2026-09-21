use super::*;

impl GpuSynth {
    /// 每块一次的声部池治理（原 `upload_voices` 前段逐语句搬移）。
    ///
    /// 负责：清零每键防风暴预算、执行独占类互斥、重建活跃组计数、
    /// 每键复音裁剪与全局声部上限裁剪。无行为变化。
    pub(crate) fn trim_voice_pool_for_block(&mut self) {
        // The per-key anti-storm guard is per block.
        self.spawn_budget.fill(0);
        // Exclusive classes resolved once per block.
        self.trim_exclusive();
        // Rebuild the active-note counts exactly (trims/exclusive kills may
        // have left the in-block counters stale). After trim_exclusive, so
        // killed classes are not counted.
        self.active_notes.fill(0);
        for v in &self.voices {
            if v.state.ended == 0 && v.release_at == u64::MAX {
                let slot = &mut self.active_notes[v.channel as usize * 128 + v.key as usize];
                *slot = slot.saturating_add(1);
            }
        }
        // Per-key polyphony trim, deferred from `spawn_voices`: ending voices
        // per note-on was O(key voices) per event (black-MIDI storms scan
        // the key for every one of thousands of notes per block); trimming
        // once per block is O(voices). Semantics mirror XSynth's
        // `pop_quietest_voice_group`: keep the `max_voices_per_key` loudest
        // note *groups* of each key (whole notes are killed, never split
        // zones), quietest first.
        let per_key_limit = self.config.max_voices_per_key;
        if per_key_limit > 0 {
            let keys: Vec<(u8, u8)> = self
                .key_voices
                .iter()
                .enumerate()
                .filter(|(_, positions)| {
                    if positions.is_empty() {
                        return false;
                    }
                    // Count distinct note groups (XSynth semantics), not voices
                    let mut groups = 0usize;
                    let mut last_nid: Option<u64> = None;
                    for &pos in positions.iter() {
                        if let Some(v) = self.voices.get(pos)
                            && Some(v.note_id) != last_nid
                        {
                            groups += 1;
                            last_nid = Some(v.note_id);
                            if groups > per_key_limit {
                                return true;
                            }
                        }
                    }
                    false
                })
                .map(|(idx, _)| ((idx / 128) as u8, (idx % 128) as u8))
                .collect();
            for (ch, key) in keys {
                self.trim_key_voices(ch, key, per_key_limit, None);
            }
            self.voices.retain(|v| v.state.ended == 0);
            self.rebuild_key_voices();
        }

        // Drop voices that ended (state refreshed by the previous readback)
        // and rebuild the per-key index before borrowing the GPU device.
        self.voices.retain(|v| v.state.ended == 0);
        // Global voice cap (once per block, not per note-on): the physical
        // GPU pool (`max_voices + max_voices/FADE_SLOTS_FRACTION`) is the
        // hard limit - a pathological MIDI (black-MIDI note storms) would
        // otherwise run the upload past the pool buffers and crash the
        // dispatch with a wgpu validation error. Release the quietest note
        // groups until we fit - cheap here because it runs once per block,
        // not once per event.
        //
        // Voices already fading (from an earlier trim, 1 ms = one block)
        // are ended outright - their output has decayed, so ending them is
        // inaudible - while fresh trims fade out instead of hard-killing
        // (a hard kill makes a sounding voice vanish in one block, an
        // audible click/crackle).
        if self.config.max_voices != 0 {
            let pool = self.config.max_voices + self.config.max_voices / FADE_SLOTS_FRACTION;
            if self.voices.len() > pool {
                let over = self.voices.len() - pool;
                // Group voices by note in one O(n) pass (spawn order keeps one
                // note's zones adjacent), then release whole quietest groups
                // until the cap fits. The previous implementation re-scanned
                // the whole voice list per group (O(over x n)) - hundreds of ms
                // per block on black-MIDI note storms at the pool cap.
                let mut groups: Vec<(u64, u8, u64, Vec<usize>)> = Vec::new();
                for (i, v) in self.voices.iter().enumerate() {
                    match groups.last_mut() {
                        Some((_, _, note, _)) if *note == v.note_id => {}
                        _ => groups.push((v.spawn_frame, v.vel, v.note_id, Vec::new())),
                    }
                    groups.last_mut().unwrap().3.push(i);
                }
                // Oldest first: a freshly-spawned note must always sound, even
                // at extreme NPS (XSynth's steal semantics).
                groups.sort_by_key(|&(spawn, vel, _, _)| (spawn, vel));
                let fade_slots = self.config.max_voices / FADE_SLOTS_FRACTION;
                let mut fade_count = self.voices.iter().filter(|v| v.fade_out).count();
                let mut freed = 0usize;
                for (_, _, _, positions) in &groups {
                    if freed >= over {
                        break;
                    }
                    for &i in positions {
                        let v = &mut self.voices[i];
                        if v.release_at == u64::MAX {
                            if fade_count < fade_slots {
                                // Fade out instead of hard-killing: a hard kill
                                // makes a sounding voice vanish in one block,
                                // an audible click/crackle. A 1 ms linear fade
                                // (XSynth's `ReleaseType::Kill`) keeps the
                                // output continuous and the voice ends right
                                // after, so the pool does not accumulate tails.
                                v.release_at = self.global_frame;
                                v.released = true;
                                v.fade_out = true;
                                v.damper_pending = false;
                                fade_count += 1;
                            } else {
                                // The fade slots are full (sustained overload):
                                // end the voice now. It is the OLDEST survivor
                                // (the sort above), so its output is already
                                // decaying - inaudible.
                                v.state.ended = 1;
                                v.damper_pending = false;
                            }
                        } else {
                            // Already fading: end it now (output has decayed).
                            v.state.ended = 1;
                            v.damper_pending = false;
                        }
                        freed += 1;
                    }
                }
                // NOTE: no retain here - the released voices stay in the pool
                // until their release envelope ends (then the GPU marks them
                // `ended` and the next block's readback prunes them).
            }
        }

        // Upload-capacity fallback: the physical pool buffers cannot hold
        // more than `pool` voices, and fading voices legitimately keep
        // occupying slots until their 1 ms fade completes. If the total
        // (active + fading) still exceeds the pool, end fading voices -
        // their output has already decayed, so this is inaudible. In the
        // pathological case where even the active voices alone exceed the
        // pool, end those too (order is preserved for the id-based state
        // resume).
        // Disabled in unlimited mode: buffers grow instead of trimming.
        if self.config.max_voices != 0 {
            // Measure against the voices still *alive* (not already marked
            // ended by the cap above). The first cap may have ended a large
            // fraction of the overflow, so the raw `voices.len()` would still
            // report the full pre-trim count and `kill` would be recomputed
            // from it - ending the survivors a second time and wiping the
            // entire pool at extreme polyphony (the "silence at high note
            // count" bug: a black-MIDI storm spawns far more voices in one
            // block than the pool, and the double-kill left zero voices).
            let alive = self.voices.iter().filter(|v| v.state.ended == 0).count();
            let pool = self.config.max_voices + self.config.max_voices / FADE_SLOTS_FRACTION;
            if alive > pool {
                let mut kill = alive - pool;
                for v in self.voices.iter_mut() {
                    if v.fade_out && kill > 0 {
                        v.state.ended = 1;
                        kill -= 1;
                    }
                }
                // Pathological: active voices alone exceed the pool. Kill
                // the OLDEST (spawn_frame), keeping fresh notes sounding.
                if kill > 0 {
                    let mut idx: Vec<usize> = (0..self.voices.len())
                        .filter(|&i| self.voices[i].state.ended == 0)
                        .collect();
                    idx.sort_by_key(|&i| (self.voices[i].spawn_frame, self.voices[i].vel));
                    for i in idx {
                        if kill > 0 {
                            self.voices[i].state.ended = 1;
                            kill -= 1;
                        }
                    }
                }
            }
        }
        self.voices.retain(|v| v.state.ended == 0);

        self.rebuild_key_voices();
    }
}
