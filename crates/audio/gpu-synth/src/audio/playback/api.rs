use super::*;

impl AudioPlayback {
    /// Plays a full, sample-accurate event stream (from
    /// [`MidiFile::load`]) in real time.
    ///
    /// The events are consumed internally by the render thread's block
    /// progression, so this scales to dense black-MIDI (millions of note
    /// events) without per-event channel traffic. The render thread ends by
    /// itself once the stream is exhausted and the voices decay.
    pub fn play_events(&mut self, events: Vec<crate::midi::TimedEvent>) {
        if let Some(tx) = &self.stream_tx {
            let _ = tx.send(events);
        }
    }

    /// Returns a snapshot reader of the playback statistics.
    ///
    /// Useful for printing a live status line (voice count / buffer /
    /// render load) while playing.
    pub fn stats(&self) -> PlaybackStatsReader {
        self.stats.clone()
    }

    /// Returns a cloneable handle to the realtime event sender.
    ///
    /// Lumino's `OutputConnection` uses this to inject MIDI events into the
    /// render thread without owning the `AudioPlayback` (which also owns the
    /// cpal audio stream). Returns `None` once playback has been stopped.
    pub fn event_sender(&self) -> Option<mpsc::Sender<(u8, MidiEvent)>> {
        self.event_tx.clone()
    }

    /// Lists the sample rates the default output device supports (empty if
    /// the device cannot be queried).
    pub fn device_sample_rates() -> Vec<u32> {
        let host = cpal::default_host();
        let Some(device) = host.default_output_device() else {
            return Vec::new();
        };
        let mut rates = Vec::new();
        if let Ok(iter) = device.supported_output_configs() {
            for cfg in iter {
                rates.push(cfg.min_sample_rate().0);
                rates.push(cfg.max_sample_rate().0);
            }
        }
        rates.sort_unstable();
        rates.dedup();
        rates
    }

    /// Sends a MIDI event to the engine (applied at the next block).
    pub fn send_event(&mut self, channel: u8, event: MidiEvent) {
        if let Some(tx) = &self.event_tx {
            let _ = tx.send((channel, event));
        }
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

    /// Convenience: sends a program change (instrument selection).
    pub fn program_change(&mut self, channel: u8, program: u8) {
        self.send_event(channel, MidiEvent::ProgramChange { program });
    }

    /// Convenience: sends a pitch bend. `value` is the raw 14-bit value
    /// (0-16383, 8192 = center).
    pub fn pitch_bend(&mut self, channel: u8, value: u16) {
        self.send_event(channel, MidiEvent::PitchBend { value });
    }

    /// Sends a control change to a channel with 14-bit MSB/LSB splitting
    /// (e.g. CC1/CC33 for vibrato depth).
    pub fn control_change_14bit(&mut self, channel: u8, msb: u8, lsb: u8, value: u16) {
        let hi = (value >> 7) as u8 & 0x7F;
        let lo = (value & 0x7F) as u8;
        self.control_change(channel, msb, hi);
        self.control_change(channel, lsb, lo);
    }

    /// Damper pedal (CC64): `down` holds all released notes until lifted.
    pub fn damper(&mut self, channel: u8, down: bool) {
        self.control_change(channel, 0x40, if down { 127 } else { 0 });
    }

    /// All notes off (CC123): releases every note on the channel.
    pub fn all_notes_off(&mut self, channel: u8) {
        self.control_change(channel, 0x7B, 0);
    }

    /// All sounds off (CC120): kills every voice on the channel instantly.
    pub fn all_sounds_off(&mut self, channel: u8) {
        self.control_change(channel, 0x78, 0);
    }

    /// Reset all controllers (CC121).
    pub fn reset_controllers(&mut self, channel: u8) {
        self.control_change(channel, 0x79, 0);
    }

    /// The device sample rate in use.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// The engine's render sample rate (events scheduled against this).
    pub fn engine_sample_rate(&self) -> u32 {
        self.engine_rate
    }

    /// Returns true while the render thread is still alive (playing).
    pub fn thread_running(&self) -> bool {
        self.thread.is_some()
    }

    /// Stops the render thread (and closes the audio stream).
    pub fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        if let Some(h) = self.thread.take() {
            let _ = h.join();
        }
        if let Some(h) = self._stream_owner.take() {
            let _ = h.join();
        }
        self.event_tx = None;
        self.stream_tx = None;
    }
}
