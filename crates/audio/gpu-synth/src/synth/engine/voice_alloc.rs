use super::*;

impl GpuSynth {
    pub(crate) fn release_key(&mut self, ch: usize, key: u8, at: u64) -> Result<(), SynthError> {
        // O(1) bail-out for orphan note-offs (no active note group on this
        // key): black-MIDI peaks fire hundreds of thousands of these per
        // block and the key scan below would dominate the block time.
        // Flame graph: `std::env::var` per note-off cost ~12% of apply
        // (200k calls/block). Cache it once.
        static NO_ACTIVE_BYPASS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        let bypass = *NO_ACTIVE_BYPASS.get_or_init(|| std::env::var("LUMINO_NO_ACTIVE").is_ok());
        if !bypass && self.active_notes[ch * 128 + key as usize] == 0 {
            return Ok(());
        }
        let damper = self.channels[ch].damper;
        // Indexed by (channel, key): only the voices of this key are touched.
        //
        // XSynth releases exactly one note per NoteOff - the *oldest* note
        // not yet releasing (FIFO, `release_next_voice`), and it releases
        // the whole note *group* (all zone voices spawned by that note-on)
        // at once. Releasing every voice of the key would cut newer notes
        // early; releasing a single zone would split a stereo pair.
        let idx = ch * 128 + key as usize;
        let positions = &self.key_voices[idx];
        let Some(nid) = select_release_note_id(&self.voices, positions.iter().copied()) else {
            return Ok(());
        };
        if damper {
            // 踏板踩下：NoteOff 只登记"待释放"，音符继续延音到踏板松开
            // （XSynth `held_by_damper`）。已登记的组会被后续 NoteOff 跳过，
            // FIFO 配对不串位。
            for &pos in positions {
                if let Some(v) = self.voices.get_mut(pos)
                    && v.note_id == nid
                    && !v.released
                    && v.release_at == u64::MAX
                {
                    v.damper_pending = true;
                }
            }
            return Ok(());
        }
        let mut released_any = false;
        for &pos in positions {
            if let Some(v) = self.voices.get_mut(pos)
                && v.note_id == nid
                && v.release_at == u64::MAX
            {
                v.release_at = at;
                v.damper_pending = false;
                released_any = true;
            }
        }
        if released_any {
            // The whole note group is now releasing; decrement the
            // active-note count (never below 0 - trims may have
            // stale-counted).
            let slot = &mut self.active_notes[idx];
            *slot = slot.saturating_sub(1);
        }
        Ok(())
    }

    pub(crate) fn spawn_voices(
        &mut self,
        ch: usize,
        key: u8,
        vel: u8,
        at: u64,
    ) -> Result<(), SynthError> {
        // The per-key note-on budget is enforced in `handle_event` before
        // this call (see `MidiEvent::NoteOn`), so this path only runs for
        // notes that may actually be heard.
        let sf = self.sf.as_ref().ok_or_else(|| {
            SynthError::Config("no soundfont loaded; call load_soundfont first".into())
        })?;
        let pitch_mult = self.channels[ch].pitch_multiplier;
        // Envelope-modifier CC values also shape the template (attack/release
        // re-parameterization happens at build time).
        let env_attack = self.channels[ch].env_attack;
        let env_release = self.channels[ch].env_release;
        let tmpl_key = (
            key,
            vel,
            ch as u8,
            pitch_mult.to_bits(),
            env_attack.unwrap_or(0xFF),
            env_release.unwrap_or(0xFF),
        );
        // Build (or reuse) the voices for this note. Template hits are the
        // hot path on black-MIDI note storms: identical notes repeat
        // thousands of times per block, and cloning skips the zone lookup +
        // envelope computation entirely. Templates are immutable; the
        // per-note fields (id, note_id, start_at, state, release) are reset
        // on clone below.
        if let Some(tmpls) = self.voice_templates.get(&tmpl_key).cloned() {
            if tmpls.is_empty() {
                return Ok(());
            }
            self.note_counter += 1;
            let note_id = self.note_counter;
            for t in &tmpls {
                let mut v = t.clone();
                v.id = self.voice_id_counter;
                self.voice_id_counter += 1;
                v.note_id = note_id;
                v.start_at = at;
                v.state = VoiceState::default();
                v.release_at = u64::MAX;
                v.released = false;
                v.damper_pending = false;
                v.sample_offset_r = 0;
                v.spawn_frame = self.global_frame;
                let pos = self.voices.len();
                self.voices.push(v);
                self.key_voices[ch * 128 + key as usize].push_back(pos);
            }
            let slot = &mut self.active_notes[ch * 128 + key as usize];
            *slot = slot.saturating_add(1);
            let limit = self.config.max_voices_per_key;
            if limit > 0 && self.key_voices[ch * 128 + key as usize].len() > limit * 2 {
                self.trim_key_voices(ch as u8, key, limit, Some(note_id));
            }
            return Ok(());
        }
        let zone_ids = sf.zones_at(key, vel).to_vec();
        let mut built: Vec<Voice> = Vec::with_capacity(zone_ids.len());
        for zone_id in zone_ids {
            if let Some(v) = build_voice(
                sf,
                zone_id,
                key,
                vel,
                ch as u8,
                at,
                self.config.sample_rate,
                pitch_mult,
                env_attack,
                env_release,
                self.config.envelope_curves,
            ) {
                built.push(v);
            }
        }
        if built.is_empty() {
            return Ok(());
        }
        self.voice_templates.insert(tmpl_key, built.clone());
        self.note_counter += 1;
        let note_id = self.note_counter;
        for mut voice in built {
            voice.id = self.voice_id_counter;
            self.voice_id_counter += 1;
            voice.note_id = note_id;
            voice.spawn_frame = self.global_frame;
            let pos = self.voices.len();
            self.voices.push(voice);
            self.key_voices[ch * 128 + key as usize].push_back(pos);
        }
        // One more active note group for this key (release_key decrements).
        let slot = &mut self.active_notes[ch * 128 + key as usize];
        *slot = slot.saturating_add(1);
        // In-block light trim: when a key's list exceeds twice its cap,
        // compact it right away. Deferring the whole trim to the block end
        // lets the list grow to tens of thousands of voices on black-MIDI
        // note storms, which makes every in-block release_key scan O(10k).
        let limit = self.config.max_voices_per_key;
        if limit > 0 && self.key_voices[ch * 128 + key as usize].len() > limit * 2 {
            self.trim_key_voices(ch as u8, key, limit, Some(note_id));
        }
        Ok(())
    }

    /// Rebuilds the per-key voice index after any mutation of `voices`
    /// (retain-based removal changes all positions).
    pub(crate) fn rebuild_key_voices(&mut self) {
        for q in self.key_voices.iter_mut() {
            q.clear();
        }
        for (i, v) in self.voices.iter().enumerate() {
            self.key_voices[v.channel as usize * 128 + v.key as usize].push_back(i);
        }
    }
}

/// 选择要被抢占的 note 组下标（按"最安静优先"排序），最多 `need_free` 个。
///
/// 规则与 XSynth `VoiceBuffer::pop_quietest_voice_group(ignored_id)` 对齐：
/// - `protected` 指定的组（通常是刚触发的 note；未指定时取 `note_id` 最大者，
///   即最新组）永不入选，保证"新音符必发声"；
/// - 其余组按 `(vel, note_id)` 升序抢占（同力度先抢旧的），结果确定。
///
/// `groups` 元素为 `(spawn_frame, vel, note_id)`；返回 `groups` 的下标，调用方
/// 负责实际释放/淡出与索引重建。
///
/// 候选不足时**宁可少抢**（返回个数可以少于 `need_free`），也绝不抢 `protected`
/// 组：`truncate` 本身就能容忍候选不足，不需要"放开保护"的兜底——那个兜底会
/// 把"新音符必发声"这条硬不变量悄悄破坏掉。
pub(crate) fn select_evictions(
    groups: &[(u64, u8, u64)],
    need_free: usize,
    protected: Option<u64>,
) -> Vec<usize> {
    if need_free == 0 || groups.is_empty() {
        return Vec::new();
    }
    let protected_id = protected.or_else(|| groups.iter().map(|&(_, _, nid)| nid).max());
    let mut candidates: Vec<usize> = (0..groups.len())
        .filter(|&i| Some(groups[i].2) != protected_id)
        .collect();
    candidates.sort_by_key(|&i| (groups[i].1, groups[i].2, i));
    candidates.truncate(need_free);
    candidates
}

/// 为一个 NoteOff 选择要释放的 note 组（FIFO，镜像 XSynth `release_next_voice`）。
///
/// 跳过已在释放中的组（`released` / `release_at != MAX`）以及"NoteOff 已到、
/// 正等待踏板松开"的组（`damper_pending`）：后者已经消费过自己的 NoteOff，
/// 后续 NoteOff 必须匹配更新的音符，否则会把老组重复配对、吞掉新音符。
/// 返回最老可释放组的 `note_id`。
pub(crate) fn select_release_note_id(
    voices: &[Voice],
    positions: impl IntoIterator<Item = usize>,
) -> Option<u64> {
    for pos in positions {
        let Some(v) = voices.get(pos) else {
            continue;
        };
        if v.released || v.release_at != u64::MAX || v.damper_pending {
            continue;
        }
        return Some(v.note_id);
    }
    None
}

/// CC64 踩→松时要释放的 note 组：`(key, note_id)`，按组去重。
///
/// 只包含"NoteOff 在踏板踩下期间到达"的组（`damper_pending`）；仍被按键
/// 按住的音符不在其中，必须继续发声（#42：旧实现把整通道在响音符全部
/// release，同 tick NoteOn/CC64=0 冲突与尾奏处会丢音）。
pub(crate) fn select_damper_release_groups(voices: &[Voice], ch: usize) -> Vec<(u8, u64)> {
    let mut groups: Vec<(u8, u64)> = Vec::new();
    for v in voices {
        if v.channel as usize != ch
            || !v.damper_pending
            || v.released
            || v.release_at != u64::MAX
            || v.state.ended != 0
        {
            continue;
        }
        let entry = (v.key, v.note_id);
        if !groups.contains(&entry) {
            groups.push(entry);
        }
    }
    groups
}
