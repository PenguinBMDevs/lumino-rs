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
        // Per-(port, key) polyphony trim (REND-002 #87 / REND-008 #94 /
        // REND-011 #105 semantics): the cap is shared by the whole port
        // (across its 16 channels) and counts EVERY sounding note group of
        // that key - including release tails. Over-cap trimming evicts
        // release tails first (they are already decaying), then the
        // quietest/oldest sustained groups; every evicted group gets the
        // 1 ms fade (`fade_out`), never a fade-less hard kill (#105).
        //
        // Why: the old per-(channel,key) cap excluded releasing voices, so
        // black-MIDI tails accumulated to 30-52k voices per block and
        // dominated the per-block upload/readback (#94). A port-level cap
        // bounds the single-port total to 128 x `max_voices_per_key` groups.
        let per_key_limit = self.config.max_voices_per_key;
        if per_key_limit > 0 {
            // One O(voices) pass buckets every live voice by (port, key).
            let mut buckets: std::collections::HashMap<(usize, u8), Vec<usize>> =
                std::collections::HashMap::new();
            for (i, v) in self.voices.iter().enumerate() {
                if v.state.ended != 0 {
                    continue;
                }
                buckets
                    .entry((v.channel as usize / 16, v.key))
                    .or_default()
                    .push(i);
            }
            // Fast path: fewer voices than the cap implies fewer groups.
            let over: Vec<(usize, u8, Vec<usize>)> = buckets
                .into_iter()
                .filter(|(_, positions)| positions.len() > per_key_limit)
                .map(|((port, key), positions)| (port, key, positions))
                .collect();
            for (port, key, positions) in over {
                self.trim_port_key_voices(port, key, &positions, per_key_limit);
            }
        }
        // Rebuild the active-note counts exactly (trims may have released
        // sustained groups; the in-block counters are stale).
        self.active_notes.fill(0);
        for v in &self.voices {
            if v.state.ended == 0 && v.release_at == u64::MAX {
                let slot = &mut self.active_notes[v.channel as usize * 128 + v.key as usize];
                *slot = slot.saturating_add(1);
            }
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

    /// 端口级每键裁剪（REND-002 #87 / #94 / #105）。
    ///
    /// `positions` 是该 `(port, key)` 的全部在响声部（跨 16 通道、含释放
    /// 尾巴）。超出 `limit` 个 note 组时：
    /// - **释放优先**：已进入释放/淡出的组先裁（输出已在衰减）；
    /// - 其次按 `(vel, note_id)` 升序裁最安静/最旧的持续组；
    /// - 保护最新组（`max note_id`）：新音符必发声；
    /// - 被裁组统一 1 ms 淡出（`fade_out`），绝不无淡出硬杀（#105）；
    ///   已在淡出的组直接 `ended`（上一块已淡完，无 click）。
    pub(crate) fn trim_port_key_voices(
        &mut self,
        port: usize,
        key: u8,
        positions: &[usize],
        limit: usize,
    ) {
        let _ = (port, key); // 索引在块末统一 retain + rebuild_key_voices
        // (releasing, spawn, vel, note_id, positions) —— 按 note_id 分组。
        let mut groups: Vec<(bool, u64, u8, u64, Vec<usize>)> = Vec::new();
        for &pos in positions {
            let Some(v) = self.voices.get(pos) else {
                continue;
            };
            if v.state.ended != 0 {
                continue;
            }
            let releasing = v.released || v.release_at != u64::MAX;
            match groups.last_mut() {
                Some((_, _, _, nid, g)) if *nid == v.note_id => g.push(pos),
                _ => groups.push((releasing, v.spawn_frame, v.vel, v.note_id, vec![pos])),
            }
        }
        let need_free = groups.len().saturating_sub(limit);
        if need_free == 0 {
            return;
        }
        // 保护最新组（整体 max note_id）；释放尾巴无需保护。
        let protected = groups.iter().map(|g| g.3).max().unwrap_or(0);
        let infos: Vec<(bool, u8, u64)> = groups.iter().map(|g| (g.0, g.2, g.3)).collect();
        let mut freed = 0usize;
        for gi in order_port_key_evictions(&infos) {
            if freed >= need_free {
                break;
            }
            if groups[gi].3 == protected {
                continue;
            }
            for &pos in &groups[gi].4 {
                let Some(v) = self.voices.get_mut(pos) else {
                    continue;
                };
                if v.state.ended != 0 {
                    continue;
                }
                if v.fade_out {
                    // 上一轮已在淡出：现在直接结束（输出已衰减，无 click）。
                    v.state.ended = 1;
                    v.damper_pending = false;
                } else {
                    // 统一 1 ms 淡出：尾巴与持续音都不硬杀（#105）。
                    v.release_at = self.global_frame;
                    v.released = true;
                    v.fade_out = true;
                    v.damper_pending = false;
                }
            }
            freed += 1;
        }
    }
}
