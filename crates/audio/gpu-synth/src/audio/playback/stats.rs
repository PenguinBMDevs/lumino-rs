use super::*;

impl PlaybackStatsReader {
    /// The number of samples currently buffered (rendered but not yet
    /// consumed). Can be negative if the reader is waiting for samples.
    pub fn samples(&self) -> i64 {
        self.samples.load(Ordering::Relaxed)
    }

    /// The number of samples requested by the last audio callback.
    pub fn last_request_samples(&self) -> i64 {
        self.last_request_samples.load(Ordering::Relaxed)
    }

    /// The number of samples that were in the buffer after the last read.
    pub fn last_samples_after_read(&self) -> i64 {
        self.last_samples_after_read.load(Ordering::Relaxed)
    }

    /// The number of samples rendered per iteration.
    pub fn render_size(&self) -> usize {
        self.render_size.load(Ordering::Relaxed) as usize
    }

    /// The average render-time percentage (0 to 1) of how long the render
    /// thread spent rendering, relative to the max allowed time. Values
    /// above 1.0 mean the render thread cannot keep up with realtime.
    pub fn average_renderer_load(&self) -> f64 {
        let head = self.render_time_head.load(Ordering::Relaxed) as usize;
        let mut sum = 0.0f64;
        let mut n = 0usize;
        for i in 0..STATS_RING {
            let bits =
                self.render_time[(head + STATS_RING - 1 - i) % STATS_RING].load(Ordering::Relaxed);
            if bits != 0 {
                sum += f64::from_bits(bits);
                n += 1;
            }
        }
        if n == 0 { 0.0 } else { sum / n as f64 }
    }

    /// The last render-time percentage (0 to 1).
    pub fn last_renderer_load(&self) -> f64 {
        let head = self.render_time_head.load(Ordering::Relaxed) as usize;
        let slot = (head + STATS_RING - 1) % STATS_RING;
        let bits = self.render_time[slot].load(Ordering::Relaxed);
        if bits == 0 { 0.0 } else { f64::from_bits(bits) }
    }

    /// Publishes one render-load sample (render thread side; lock-free).
    pub(crate) fn push_render_load(&self, load: f64) {
        let head = self.render_time_head.load(Ordering::Relaxed) as usize;
        self.render_time[head].store(load.to_bits(), Ordering::Relaxed);
        let next = (head + 1) % STATS_RING;
        self.render_time_head.store(next as u64, Ordering::Relaxed);
    }

    /// The active voice count reported by the engine on the last render.
    pub fn voice_count(&self) -> u64 {
        self.voice_count.load(Ordering::Relaxed)
    }

    /// The number of underruns (audio callbacks that found no buffered
    /// samples and wrote silence). Zero is the healthy state.
    pub fn underruns(&self) -> u64 {
        self.underruns.load(Ordering::Relaxed)
    }
}
