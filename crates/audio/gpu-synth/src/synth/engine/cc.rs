use super::*;

impl GpuSynth {
    pub(crate) fn apply_cc(&mut self, ch: usize, controller: u8, value: u8) {
        let sr = self.config.sample_rate;
        let mut pitch_dirty = false;
        match controller {
            // CC7 (volume), CC11 (expression), CC10/CC8 (pan) are handled by
            // `handle_event` -> `defer_mix_cc` for frame-exact application;
            // they never reach this function.
            0x07 | 0x0B | 0x0A | 0x08 => {
                debug_assert!(false, "CC7/11/10/8 must go through defer_mix_cc");
                let _ = sr;
            }
            0x47 => {
                // Resonance (CC71): unused by the SF2 voice path in XSynth
                // (voice resonance comes from the soundfont), but tracked for
                // completeness.
                let _ = value;
            }
            // ---- RPN / NRPN selection + Data Entry (pitch-critical) ----
            // 语义见 `ChannelState::handle_rpn_cc`：CC100/101 = RPN LSB/MSB，
            // CC98/99 = NRPN LSB/MSB，NRPN 数据消费丢弃（规范）。
            0x62..=0x65 => {
                self.channels[ch].handle_rpn_cc(controller, value);
            }
            0x06 | 0x26 => {
                if self.channels[ch].handle_rpn_cc(controller, value) {
                    pitch_dirty = true;
                }
            }
            0x48 => {
                // Release time (CC72): modifies the release envelope stage.
                self.channels[ch].env_release = Some(value);
                for v in &mut self.voices {
                    if v.channel as usize == ch {
                        v.env_release = Some(value);
                        refresh_env_stages(v);
                    }
                }
            }
            0x49 => {
                // Attack time (CC73): modifies the attack envelope stage.
                self.channels[ch].env_attack = Some(value);
                for v in &mut self.voices {
                    if v.channel as usize == ch {
                        v.env_attack = Some(value);
                        refresh_env_stages(v);
                    }
                }
            }
            0x40 => {
                let was_damper = self.channels[ch].damper;
                let damper = value >= 64;
                self.channels[ch].damper = damper;
                // 松开踏板：只释放"NoteOff 已到、但被踏板扣住"的组；仍被按键
                // 按住的音符必须继续发声。旧实现把整通道在响音符全部 release，
                // 同 tick NoteOn/CC64=0 冲突与尾奏处会偶发缺音（#42）。
                if was_damper && !damper {
                    let groups = select_damper_release_groups(&self.voices, ch);
                    if !groups.is_empty() {
                        for v in &mut self.voices {
                            if v.channel as usize == ch
                                && v.damper_pending
                                && !v.released
                                && v.release_at == u64::MAX
                                && v.state.ended == 0
                            {
                                v.release_at = self.global_frame;
                                v.damper_pending = false;
                            }
                        }
                        // 每个被释放的 note 组只减一次活跃计数。
                        for (key, _) in &groups {
                            let slot = &mut self.active_notes[ch * 128 + *key as usize];
                            *slot = slot.saturating_sub(1);
                        }
                    }
                }
            }
            0x79 => {
                // Reset all controllers.
                self.channels[ch] = ChannelState::new();
                pitch_dirty = true;
            }
            0x7B => {
                // All notes off.
                for v in &mut self.voices {
                    if v.channel as usize == ch {
                        v.release_at = self.global_frame;
                        v.damper_pending = false;
                    }
                }
            }
            0x78 => {
                // All sounds off: kill immediately.
                self.voices.retain(|v| v.channel as usize != ch);
                self.rebuild_key_voices();
            }
            _ => {}
        }
        if pitch_dirty {
            self.propagate_channel_pitch(ch);
        }
    }

    /// Recomputes a channel's `pitch_multiplier` is already done in
    /// `ChannelState`; this pushes the new multiplier onto every *active*
    /// voice of the channel so a bend-sensitivity or tuning change takes
    /// effect on sounding notes immediately (otherwise only future note-ons
    /// would track it, and held notes would sit at the old pitch).
    pub(crate) fn propagate_channel_pitch(&mut self, ch: usize) {
        if let Some(sf) = self.sf.as_ref() {
            let mult = self.channels[ch].pitch_multiplier;
            for v in &mut self.voices {
                if v.channel as usize == ch {
                    let zone = sf.zone(v.zone_id);
                    v.speed = zone.speed_mult * mult;
                }
            }
        }
    }

    /// Trims one (channel, key) voice list to the `limit` loudest note
    /// groups, releasing the rest (they fade out via their release
    /// envelope instead of being hard-killed - a hard kill makes a
    /// sounding voice vanish in one block, an audible click at the
    /// polyphony cap). Whole notes are released, never split zones -
    /// mirroring XSynth's `pop_quietest_voice_group` + `fade_out_killing`.
    ///
    /// `protected` is the note_id that must survive this trim (the group just
    /// spawned); `None` protects the newest group (max note_id). This mirrors
    /// XSynth's `ignored_id`: a fresh note always sounds, the quietest of the
    /// other groups is stolen. O(key voices), so it must only run when the
    /// key exceeds its cap, not per note-on.
    ///
    /// `limit == 0` 表示"每键不限制"（与 UI 的 `0=无限`、`spawn_budget_allows`
    /// 的 0 语义一致），此时不抢占任何组——**绝不能**让 0 走到下面的
    /// `saturating_sub`，否则"无限制"会变成"杀掉该键全部音符"。
    pub(crate) fn trim_key_voices(
        &mut self,
        ch: u8,
        key: u8,
        limit: usize,
        protected: Option<u64>,
    ) {
        if limit == 0 {
            return;
        }
        let idx = ch as usize * 128 + key as usize;
        let positions: Vec<usize> = self.key_voices[idx].iter().copied().collect();
        // Group by note_id (spawn order keeps one note's zones adjacent);
        // ended/released voices are excluded (they are already fading out).
        let mut groups: Vec<(u64, u8, u64, Vec<usize>)> = Vec::new();
        for &pos in &positions {
            let Some(v) = self.voices.get(pos) else {
                continue;
            };
            if v.state.ended != 0 || v.release_at != u64::MAX {
                continue;
            }
            match groups.last_mut() {
                Some((_, _, nid, g)) if *nid == v.note_id => g.push(pos),
                _ => groups.push((v.spawn_frame, v.vel, v.note_id, vec![pos])),
            }
        }
        // `limit` counts note GROUPS (one note = one group, all its zones).
        let need_free = groups.len().saturating_sub(limit);
        if need_free == 0 {
            return;
        }
        let infos: Vec<(u64, u8, u64)> = groups
            .iter()
            .map(|(spawn, vel, nid, _)| (*spawn, *vel, *nid))
            .collect();
        let evict = select_evictions(&infos, need_free, protected);
        // For dense black MIDI (>20k), hard-kill is inaudible (dense mix
        // masks the 1-block click) but saves 1 block of fading voices
        // (20k * 32ms tail = 640k voice-blocks). Flame showed fading
        // accumulation is the 80k→70k leak.
        let hard_kill = self.voices.len() > 20000;
        for &gi in &evict {
            for &pos in &groups[gi].3 {
                if let Some(v) = self.voices.get_mut(pos)
                    && v.release_at == u64::MAX
                    && v.state.ended == 0
                {
                    if hard_kill {
                        v.state.ended = 1;
                        v.damper_pending = false;
                    } else {
                        v.release_at = self.global_frame;
                        v.released = true;
                        v.fade_out = true;
                        v.damper_pending = false;
                    }
                }
            }
        }
        // Compact the key index: drop ended and fading entries so
        // per-event scans (release_key, further trims) stay bounded by the
        // cap. Fading voices remain in `voices` for GPU but are not per-key.
        let kept: VecDeque<usize> = positions
            .iter()
            .copied()
            .filter(|&pos| {
                self.voices
                    .get(pos)
                    .is_some_and(|v| v.state.ended == 0 && v.release_at == u64::MAX)
            })
            .collect();
        self.key_voices[idx] = kept;
        // Rebuild this key's active-note count exactly. Decrementing by the
        // killed group count would underflow: the trimmed groups include
        // already-released notes that were decremented at release time, and
        // a stale (too-low) count makes the release fast path skip live
        // notes, leaving them sustained forever.
        let mut live_notes: Vec<u64> = Vec::new();
        for &pos in &positions {
            if let Some(v) = self.voices.get(pos)
                && v.state.ended == 0
                && v.release_at == u64::MAX
                && !live_notes.contains(&v.note_id)
            {
                live_notes.push(v.note_id);
            }
        }
        self.active_notes[ch as usize * 128 + key as usize] = live_notes.len() as u32;
    }

    /// Ends every voice whose exclusive class has a newer note (the newest
    /// note of a class wins, mirroring XSynth). Runs once per block in
    /// `upload_voices` - the previous per-note-on scan was O(voices) per
    /// event and dominated black-MIDI peak blocks.
    ///
    /// Killed voices FADE OUT (1 ms, XSynth's `ReleaseType::Kill`) instead
    /// of being hard-ended: a hard `ended = 1` here makes a sounding voice
    /// vanish instantly, an audible click - and in black-MIDI exclusive
    /// storms (the same note retriggered thousands of times) that is a
    /// continuous crackle, the user's "800-1000 voices and it pops"
    /// symptom. The fade stage is the release, so the voice ends 1 ms later
    /// and the newest note still wins immediately.
    pub(crate) fn trim_exclusive(&mut self) {
        let mut newest: std::collections::HashMap<u8, u64> = std::collections::HashMap::new();
        for v in &self.voices {
            if let Some(c) = v.exclusive_class {
                newest
                    .entry(c)
                    .and_modify(|n| *n = (*n).max(v.note_id))
                    .or_insert(v.note_id);
            }
        }
        if newest.is_empty() {
            return;
        }
        for v in &mut self.voices {
            if let Some(c) = v.exclusive_class
                && v.state.ended == 0
                && v.release_at == u64::MAX
                && newest.get(&c).is_some_and(|&n| n != v.note_id)
            {
                // Fade out instead of hard-ending (see the doc above).
                v.release_at = self.global_frame;
                v.released = true;
                v.fade_out = true;
                v.damper_pending = false;
            }
        }
    }
}
