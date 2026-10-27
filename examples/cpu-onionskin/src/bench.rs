//! 基准滚动脚本：从最密小节前 2 小节开始，匀速滚过 3 小节。

use std::time::{Duration, Instant};

use crate::{camera::Camera, model::TrackIndex};

pub struct Bench {
    started: Option<Instant>,
    duration: Duration,
    start_scroll: f32,
    speed_px_per_sec: f32,
    pub finished: bool,
}

impl Bench {
    #[must_use]
    pub fn new(camera: &Camera, index: &TrackIndex, seconds: u64) -> Self {
        let measure_ticks = index.measure_ticks.max(1);
        let start_tick = (index.densest_measure as u32)
            .saturating_mul(measure_ticks)
            .saturating_sub(measure_ticks.saturating_mul(2));
        let start_scroll = (start_tick as f32 * camera.zoom_x).clamp(0.0, camera.max_scroll_x());
        let duration_secs = seconds.max(1) as f32;
        let span_px = measure_ticks as f32 * camera.zoom_x * 3.0;
        Self {
            started: None,
            duration: Duration::from_secs(seconds.max(1)),
            start_scroll,
            speed_px_per_sec: span_px / duration_secs,
            finished: false,
        }
    }

    pub fn start_scroll(&self) -> f32 {
        self.start_scroll
    }

    pub fn set_start(&mut self, scroll: f32) {
        self.start_scroll = scroll;
        self.speed_px_per_sec = 0.0;
    }

    pub fn apply(&mut self, camera: &mut Camera) -> bool {
        if self.finished {
            return false;
        }
        let started = *self.started.get_or_insert_with(Instant::now);
        let elapsed = started.elapsed().as_secs_f32();
        camera.scroll_x =
            (self.start_scroll + self.speed_px_per_sec * elapsed).clamp(0.0, camera.max_scroll_x());
        camera.target_scroll_x = camera.scroll_x;
        if elapsed >= self.duration.as_secs_f32() {
            self.finished = true;
            return false;
        }
        true
    }
}
