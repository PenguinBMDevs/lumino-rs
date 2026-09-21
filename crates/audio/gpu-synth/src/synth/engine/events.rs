use super::*;

impl GpuSynth {
    /// Queues a MIDI event (applied at the next block boundary).
    pub fn send_event(&mut self, channel: u8, event: MidiEvent) {
        self.pending_events.push_back(TimedEvent::from_event(
            self.global_frame as u32,
            channel.min(15),
            event,
        ));
    }

    /// Loads a full sample-accurate event stream for realtime playback.
    ///
    /// The render thread consumes it internally (like the offline renderer)
    /// by wall-clock-progressed `global_frame`, so the events never travel
    /// through the per-event channel — the only way to keep up with dense
    /// black-MIDI (hundreds of thousands of note events per second).
    pub fn set_events(&mut self, events: Vec<TimedEvent>) {
        self.offline_cursor = 0;
        self.offline_events = events;
    }

    /// Returns true when the loaded event stream has been fully consumed and
    /// no voice is still sounding (used by realtime playback to end the
    /// render thread on its own).
    pub fn stream_exhausted(&self) -> bool {
        if self.offline_cursor < self.offline_events.len() {
            return false;
        }
        self.voices.is_empty()
    }

    /// Convenience: sends a note-on.
    pub fn note_on(&mut self, channel: u8, key: u8, vel: u8) {
        self.send_event(channel, MidiEvent::NoteOn { key, vel });
    }

    /// Convenience: sends a note-off.
    pub fn note_off(&mut self, channel: u8, key: u8) {
        self.send_event(channel, MidiEvent::NoteOff { key });
    }

    /// Convenience: sends a control change.
    pub fn control_change(&mut self, channel: u8, controller: u8, value: u8) {
        self.send_event(channel, MidiEvent::ControlChange { controller, value });
    }

    // ------------------------------------------------------------------
    // Block rendering
    // ------------------------------------------------------------------

    pub(crate) fn apply_events(&mut self, _base: u64, end: u64) -> Result<(), SynthError> {
        // Real-time queue first.
        while let Some(ev) = self.pending_events.pop_front() {
            self.handle_event(ev)?;
        }
        // Offline event stream (events with sample < end belong to this block).
        while self.offline_cursor < self.offline_events.len() {
            let ev = self.offline_events[self.offline_cursor];
            if ev.sample as u64 >= end {
                break;
            }
            self.offline_cursor += 1;
            self.handle_event(ev)?;
        }
        Ok(())
    }

    pub(crate) fn handle_event(&mut self, ev: TimedEvent) -> Result<(), SynthError> {
        let ch = ev.channel() as usize;
        // Fast path on packed kind/payload to avoid constructing MidiEvent
        // enum for the hot note-on/note-off path (black MIDI: >1M events/sec).
        match ev.kind() {
            crate::midi::kind::NOTE_ON => {
                let payload = ev.payload();
                let key = payload as u8;
                let vel = (payload >> 8) as u8;
                // Velocity 0 is converted to a note-off by the parser; a
                // velocity of 1 is a barely-audible note that XSynth does
                // not render. Dropping it saves a voice slot without any
                // audible change.
                if vel <= 1 {
                    return Ok(());
                }
                // Per-key anti-storm guard, checked HERE so a pathological
                // burst (millions of note-ons on one key in one block) skips
                // the spawn call entirely. This is NOT the per-key polyphony
                // limit: every admitted note-on must sound, and
                // `trim_key_voices` steals the quietest OLD group afterwards
                // (XSynth semantics). The old per-`max_voices_per_key`
                // budget dropped the NEWEST notes and broke dense passages.
                let slot = &mut self.spawn_budget[ch * 128 + key as usize];
                if !spawn_budget_allows(*slot) {
                    return Ok(());
                }
                *slot += 1;
                // Global pool gate: bound per-block spawn cost WITHOUT dropping
                // audible notes. `upload_voices` trims the pool down to `pool`
                // keeping the OLDEST voices, so a new note-on must still be
                // spawned even when the pool is already full - it steals an
                // older voice and sounds. We therefore only stop spawning once
                // we are a full pool ABOVE the cap, which leaves enough
                // headroom for the per-key loudest-pick and the global
                // oldest-survives steal while capping event processing at
                // ~2*pool spawns/block instead of the unbounded note-on burst
                // that dominated `apply_events` (and starved the realtime
                // queue). Notes past the `2*pool` bound would be trimmed away
                // by the pool cap anyway, so nothing audible is lost.
                // In unlimited mode (max_voices == 0) every note must sound, so
                // the gate is disabled - but we still cap per-block spawns to
                // a very large budget (200k) to avoid a pathological 10M note
                // burst stalling the render thread for seconds.
                if self.config.max_voices != 0 {
                    let pool =
                        self.config.max_voices + self.config.max_voices / FADE_SLOTS_FRACTION;
                    if self.voices.len() >= pool + pool {
                        return Ok(());
                    }
                } else if self.voices.len() >= 400_000 {
                    // Unlimited but still bound the worst-case single-block burst
                    // to keep the block time bounded; black MIDI peaks are far
                    // below this, so nothing audible is lost.
                    return Ok(());
                }
                self.spawn_voices(ch, key, vel, ev.sample as u64)
            }
            crate::midi::kind::NOTE_OFF => {
                let key = ev.payload() as u8;
                self.release_key(ch, key, ev.sample as u64)
            }
            crate::midi::kind::CONTROL_CHANGE => {
                let payload = ev.payload();
                let controller = payload as u8;
                let value = (payload >> 8) as u8;
                match controller {
                    // Channel mix controllers: deferred for frame-exact
                    // application at the mix stage.
                    0x07 | 0x0B | 0x0A | 0x08 => {
                        self.pending_mix_events.push((
                            ev.sample as u64,
                            ch as u8,
                            controller,
                            value,
                        ));
                    }
                    _ => self.apply_cc(ch, controller, value),
                }
                Ok(())
            }
            crate::midi::kind::PROGRAM_CHANGE => {
                let program = ev.payload() as u8;
                self.channels[ch].program = program.min(127);
                Ok(())
            }
            crate::midi::kind::PITCH_BEND => {
                let value = ev.payload() as u16;
                // 14-bit value: 0..16383, center 8192. The sensitivity comes
                // from RPN 0 (Pitch Bend Sensitivity), defaulting to 2
                // semitones; store the raw value and recompute so a later RPN
                // change re-scales the held bend correctly.
                self.channels[ch].bend_value = value as i32;
                self.channels[ch].recompute_pitch();
                self.propagate_channel_pitch(ch);
                Ok(())
            }
            _ => Ok(()),
        }
    }
}
