/// A single-line `\r`-rewritten progress bar for offline rendering.
///
/// The bar shows the fraction of the render horizon that is complete. It is
/// a no-op when disabled (library callers), and rewrites one line on stderr
/// so long exports stay visibly alive without flooding the log.
pub(crate) struct ProgressBar {
    /// Total frame count the bar is measured against.
    total: u64,
    /// Progress bar width in characters.
    width: usize,
    /// Last reported percent, so the bar only repaints when it changes.
    last_pct: i32,
    /// Whether output is enabled at all.
    enabled: bool,
}

impl ProgressBar {
    pub(crate) fn new(total: u64, enabled: bool) -> Self {
        Self {
            total: total.max(1),
            width: 24,
            last_pct: -1,
            enabled,
        }
    }

    /// Advances the bar to `done` frames and repaints when the percent
    /// crossed a whole-number boundary.
    pub(crate) fn tick(&mut self, done: u64) {
        if !self.enabled {
            return;
        }
        let pct = ((done as f64 / self.total as f64) * 100.0) as i32;
        if pct <= self.last_pct {
            return;
        }
        self.last_pct = pct;
        self.paint(pct);
    }

    /// Ends the bar on its own line (100% or the last painted value).
    pub(crate) fn finish(&mut self) {
        if !self.enabled {
            return;
        }
        self.last_pct = 100;
        self.paint(100);
        eprintln!();
    }

    pub(crate) fn paint(&self, pct: i32) {
        let pct = pct.clamp(0, 100);
        let filled = (pct as usize * self.width) / 100;
        let bar: String = std::iter::repeat_n('=', filled)
            .chain(std::iter::repeat_n(' ', self.width - filled))
            .collect();
        eprint!("\r[render] [{}] {pct:3}%", bar);
    }
}
