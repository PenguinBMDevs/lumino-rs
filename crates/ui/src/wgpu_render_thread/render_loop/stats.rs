use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::super::params::RenderParams;
use super::super::stats::RenderStats;

/// 更新渲染统计
pub fn update_stats(
    frame_count: &mut u64,
    fps_update_time: &mut Instant,
    frame_time: Duration,
    params: &RenderParams,
    stats_clone: &Arc<Mutex<RenderStats>>,
) {
    *frame_count += 1;

    let elapsed = fps_update_time.elapsed();
    if elapsed.as_secs() >= 1 {
        if let Ok(mut stats) = stats_clone.lock() {
            stats.frame_count = *frame_count;
            stats.last_frame_time_ms = frame_time.as_secs_f64() * 1000.0;
            stats.average_fps = *frame_count as f64 / elapsed.as_secs_f64();
            stats.note_count = params.note_instances.len();
            stats.grid_line_count = params.grid_instances.len();
            stats.key_count = params.keyboard_instances.len();
            stats.ruler_tick_count = params.ruler_instances.len();
        }
        *fps_update_time = Instant::now();
        *frame_count = 0;
    }
}
