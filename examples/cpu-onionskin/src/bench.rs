//! 基准脚本：匀速滚动 / 快速大范围缩放。
//!
//! 滚动基准从最密小节前 2 小节开始匀速滚过 3 小节；缩放基准以最密小节中心为
//! 锚点，在对数空间里从全曲适配缩放到高倍逐音符缩放来回扫，强制每帧走
//! "相机变化 → 全量重绘"的最坏路径。

use std::time::{Duration, Instant};

use crate::{camera::Camera, model::TrackIndex};

#[derive(Debug, Clone, Copy)]
enum Kind {
    Scroll,
    Zoom,
}

pub struct Bench {
    kind: Kind,
    started: Option<Instant>,
    duration: Duration,
    start_scroll: f32,
    speed_px_per_sec: f32,
    // 缩放基准
    center_tick: f32,
    center_x: f32,
    low_zoom: f32,
    high_zoom: f32,
    cycles: f32,
    pub finished: bool,
}

impl Bench {
    /// 匀速水平滚动基准。
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
            kind: Kind::Scroll,
            started: None,
            duration: Duration::from_secs(seconds.max(1)),
            start_scroll,
            speed_px_per_sec: span_px / duration_secs,
            center_tick: 0.0,
            center_x: 0.0,
            low_zoom: camera.zoom_x,
            high_zoom: camera.zoom_x,
            cycles: 0.0,
            finished: false,
        }
    }

    /// 快速大范围缩放基准。
    ///
    /// 锚点 = 最密小节中心；缩放在对数空间从全曲适配（至少能看见整曲）扫到
    /// `high_zoom`，每 `cycles` 个来回占满 `seconds`。每帧 `zoom_x` 都变化，
    /// 因此不会命中滚动增量路径，只会走全量 Exact/Overview。
    #[must_use]
    pub fn new_zoom(
        camera: &Camera,
        index: &TrackIndex,
        seconds: u64,
        high_zoom: f32,
        cycles: f32,
        low_override: Option<f32>,
    ) -> Self {
        let measure_ticks = index.measure_ticks.max(1) as f32;
        let center_tick = index.densest_measure as f32 * measure_ticks + measure_ticks * 0.5;
        let fit = camera.canvas_width() / camera.total_ticks.max(1) as f32;
        let high_zoom = high_zoom.max(0.0002);
        let low_zoom = low_override.unwrap_or(fit).clamp(0.0002, high_zoom);
        let center_x = camera.width as f32 * 0.5;
        let max_start_scroll =
            (camera.total_ticks as f32 * low_zoom - camera.canvas_width()).max(0.0);
        let start_scroll = (center_tick * low_zoom - (center_x - camera.keyboard_width))
            .clamp(0.0, max_start_scroll);
        Self {
            kind: Kind::Zoom,
            started: None,
            duration: Duration::from_secs(seconds.max(1)),
            start_scroll,
            speed_px_per_sec: 0.0,
            center_tick,
            center_x,
            low_zoom,
            high_zoom,
            cycles: cycles.max(1.0),
            finished: false,
        }
    }

    /// 缩放基准的起始缩放。
    #[must_use]
    pub fn start_zoom(&self) -> f32 {
        self.low_zoom
    }

    /// 起始滚动（滚动/缩放共用）。
    #[must_use]
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
        if elapsed >= self.duration.as_secs_f32() {
            self.finished = true;
            return false;
        }
        match self.kind {
            Kind::Scroll => {
                camera.scroll_x = (self.start_scroll + self.speed_px_per_sec * elapsed)
                    .clamp(0.0, camera.max_scroll_x());
            }
            Kind::Zoom => {
                let progress = (elapsed / self.duration.as_secs_f32()).clamp(0.0, 1.0);
                // 0 → 1 → 0 的余弦缓动；每条腿都覆盖完整缩放范围。
                let phase = progress * self.cycles * std::f32::consts::TAU;
                let t = 0.5 - 0.5 * phase.cos();
                let zoom =
                    (self.low_zoom.ln() + (self.high_zoom.ln() - self.low_zoom.ln()) * t).exp();
                camera.zoom_x = zoom.clamp(0.0002, 40.0);
                // 以固定屏幕锚点保持同一 tick 在光标下，和交互缩放口径一致。
                camera.scroll_x = (self.center_tick * camera.zoom_x
                    - (self.center_x - camera.keyboard_width))
                    .clamp(0.0, camera.max_scroll_x());
            }
        }
        camera.target_scroll_x = camera.scroll_x;
        true
    }
}
