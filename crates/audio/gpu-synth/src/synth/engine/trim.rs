use super::*;

impl GpuSynth {
    /// 每块一次的声部池治理（原 `upload_voices` 前段逐语句搬移）。
    ///
    /// 负责：清零每键防风暴预算、执行独占类互斥、重建活跃组计数、
    /// 每键复音裁剪与全局声部上限裁剪。
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
        // 5 ms fade (`fade_out`, REND-015 #115), never a fade-less hard
        // kill (#105). Trimming only triggers once the bucket exceeds
        // `cap + trim_hysteresis(cap)`, and new fades are bounded per block
        // by `trim_fade_budget` so dense passages cannot stack thousands of
        // simultaneous fades into a crackle; the excess is deferred to
        // later blocks instead of being cut in one batch.
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
            // 触发阈值含迟滞余量（cap + max(1, cap/4)），避免 cap 边界上
            // 每块 1-2 个组的小批量持续抢（REND-015 #115）。
            let threshold = per_key_limit + trim_hysteresis(per_key_limit);
            let over: Vec<(usize, u8, Vec<usize>)> = buckets
                .into_iter()
                .filter(|(_, positions)| positions.len() > threshold)
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
        // Voices already fading (from an earlier trim; the 5 ms fade may
        // span blocks) are only ended once the fade has actually completed -
        // their output has decayed, so ending them is inaudible - while
        // fresh trims fade out instead of hard-killing (a hard kill makes a
        // sounding voice vanish in one block, an audible click/crackle).
        //
        // This path only runs with a global `max_voices` cap (live LGS and
        // GPU export both use unlimited = 0). New fades here are not taken
        // from `trim_fade_budget`: the global cap is a physical safety limit
        // and must still converge; the fade-slot bound below keeps the
        // hard-kill escape hatch for truly overloaded pools.
        let fade_len = fade_frames(self.config.sample_rate);
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
                                // an audible click/crackle. A 5 ms linear fade
                                // (REND-015 #115, aligned with XSynth's
                                // `ReleaseType::Kill`) keeps the output
                                // continuous and the voice ends right after,
                                // so the pool does not accumulate tails.
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
                        } else if fade_complete(true, v.release_at, self.global_frame, fade_len) {
                            // Already faded out: end it now (output has
                            // decayed, inaudible). A still-fading voice is
                            // kept and ends on its own once the 5 ms fade
                            // completes.
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
        // occupying slots until their 5 ms fade completes. If the total
        // (active + fading) still exceeds the pool, end fading voices -
        // already-completed fades first (inaudible), then still-fading ones
        // (their output is already decaying). In the pathological case where
        // even the active voices alone exceed the pool, end those too (order
        // is preserved for the id-based state resume).
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
                // 1) 已淡完的最先释放：无 click。
                for v in self.voices.iter_mut() {
                    if kill == 0 {
                        break;
                    }
                    if v.state.ended == 0
                        && v.fade_out
                        && fade_complete(true, v.release_at, self.global_frame, fade_len)
                    {
                        v.state.ended = 1;
                        kill -= 1;
                    }
                }
                // 2) 物理池仍不够：牺牲未淡完的淡出声部（其输出已在衰减，
                //    且这是避免 wgpu 越界的安全网）。
                for v in self.voices.iter_mut() {
                    if kill == 0 {
                        break;
                    }
                    if v.state.ended == 0 && v.fade_out {
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

    /// 端口级每键裁剪（REND-002 #87 / #94 / #105；REND-015 #115 限流）。
    ///
    /// `positions` 是该 `(port, key)` 的全部在响声部（跨 16 通道、含释放
    /// 尾巴）。超出 `limit` 个 note 组时：
    /// - **释放优先**：已进入释放/淡出的组先裁（输出已在衰减）；
    /// - 其次按 `(vel, note_id)` 升序裁最安静/最旧的持续组；
    /// - 保护最新组（`max note_id`）：新音符必发声；
    /// - 被裁组统一 **5 ms** 淡出（`fade_out`），绝不无淡出硬杀（#105）；
    /// - 已在淡出的组只有淡出播完才 `ended`（5 ms 可能跨块，绝不中途硬切）；
    /// - 每块新建淡出受共享预算 `trim_fade_budget` 限制（REND-015 #115），
    ///   超出的组留待后续块；已在淡出的组不占预算（顺手结束已播完的）。
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
        // REND-015 #115 每块预算：组粒度取用，超出的组留待后续块。
        let max_groups = trimmed_group_count(need_free, self.trim_fade_budget);
        let fade_len = fade_frames(self.config.sample_rate);
        let mut started = 0usize;
        let mut freed = 0usize;
        for gi in order_port_key_evictions(&infos) {
            if freed >= max_groups {
                break;
            }
            if groups[gi].3 == protected {
                continue;
            }
            // 整组一起淡出：预算在组粒度判断，避免拆开一个 note 的多个 zone。
            let group_fading = groups[gi].4.iter().any(|&pos| {
                self.voices
                    .get(pos)
                    .is_some_and(|v| v.state.ended == 0 && v.fade_out)
            });
            if group_fading {
                // 已在淡出的组不占预算：只顺手结束已经播完的；未播完的留给
                // 自己自然结束（5 ms 内绝不硬切）。
                for &pos in &groups[gi].4 {
                    let Some(v) = self.voices.get_mut(pos) else {
                        continue;
                    };
                    if v.state.ended == 0
                        && fade_complete(true, v.release_at, self.global_frame, fade_len)
                    {
                        v.state.ended = 1;
                        v.damper_pending = false;
                    }
                }
            } else {
                // 统一 5 ms 淡出：尾巴与持续音都不硬杀（#105）。
                for &pos in &groups[gi].4 {
                    let Some(v) = self.voices.get_mut(pos) else {
                        continue;
                    };
                    if v.state.ended != 0 {
                        continue;
                    }
                    v.release_at = self.global_frame;
                    v.released = true;
                    v.fade_out = true;
                    v.damper_pending = false;
                }
                started += 1;
            }
            freed += 1;
        }
        self.trim_fade_budget = self.trim_fade_budget.saturating_sub(started);
    }
}
