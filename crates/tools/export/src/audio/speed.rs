//! 导出倍速计：用短窗口平均估算"当前导出速度 = 实时播放的多少倍"。
//!
//! 采用滑动窗口而不是瞬时差分：黑 MIDI 的单块耗时波动很大，瞬时值会出现
//! 离谱的跳变；窗口平均既平滑又能在数秒内反映速度变化。

use std::collections::VecDeque;

/// 默认窗口长度（秒）。
pub const DEFAULT_SPEED_WINDOW_SECS: f64 = 3.0;

/// 求窗口内平均倍速所需的最短时间跨度（秒）：更短时返回 `None`，
/// 避免导出刚启动的几帧显示出夸张的倍速。
const MIN_SPAN_SECS: f64 = 0.5;

/// 导出倍速计（音频秒 / 墙钟秒）。
///
/// 调用方在每次进度更新时调用 [`ExportSpeedMeter::record`]，
/// 用相同单位（秒）记录墙钟时间与已完成的音频时长。
pub struct ExportSpeedMeter {
    window_secs: f64,
    /// `(墙钟秒, 音频秒)`，按时间升序。
    samples: VecDeque<(f64, f64)>,
}

impl ExportSpeedMeter {
    /// 创建指定窗口长度（秒）的倍速计。
    pub fn new(window_secs: f64) -> Self {
        Self {
            window_secs: window_secs.max(0.1),
            samples: VecDeque::new(),
        }
    }

    /// 记录一次进度。`now_secs` 与 `audio_secs_done` 都以秒为单位，
    /// 且 `now_secs` 必须非递减。
    pub fn record(&mut self, now_secs: f64, audio_secs_done: f64) {
        self.samples.push_back((now_secs, audio_secs_done));
        // 保留窗口起点之前的一个锚点，保证差分区间略大于等于窗口长度。
        while self.samples.len() > 2 {
            let (t1, _) = self.samples[1];
            if now_secs - t1 >= self.window_secs {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    /// 窗口内平均倍速；样本时间跨度不足时返回 `None`。
    pub fn speed(&self) -> Option<f64> {
        let &(t0, a0) = self.samples.front()?;
        let &(t1, a1) = self.samples.back()?;
        let dt = t1 - t0;
        if dt < MIN_SPAN_SECS {
            return None;
        }
        Some(((a1 - a0).max(0.0)) / dt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_needs_two_samples_and_a_minimum_span() {
        let mut meter = ExportSpeedMeter::new(2.0);
        assert_eq!(meter.speed(), None);
        meter.record(0.0, 0.0);
        assert_eq!(meter.speed(), None, "单样本不足以求速度");
        meter.record(0.2, 1.0);
        assert_eq!(meter.speed(), None, "时间跨度 < 0.5s 不应给出速度");
    }

    #[test]
    fn speed_averages_over_window() {
        let mut meter = ExportSpeedMeter::new(2.0);
        meter.record(0.0, 0.0);
        meter.record(1.0, 10.0);
        assert_eq!(meter.speed(), Some(10.0));
        // 1 秒只推进 1 秒音频的抖动被窗口平滑
        meter.record(2.0, 11.0);
        let s = meter.speed().expect("窗口内应可求速度");
        assert!((s - 5.5).abs() < 1e-9, "窗口锚点应为 0s，得到 {s}");
        // 窗口滚动：锚点推进到 1s，得到 (12-10)/(3-1)=1.0
        meter.record(3.0, 12.0);
        let s = meter.speed().expect("窗口内应可求速度");
        assert!((s - 1.0).abs() < 1e-9, "滚动后窗口应变为 1s..3s，得到 {s}");
    }

    #[test]
    fn speed_never_goes_negative() {
        let mut meter = ExportSpeedMeter::new(2.0);
        meter.record(0.0, 5.0);
        meter.record(1.0, 4.0); // 异常回退输入
        assert_eq!(meter.speed(), Some(0.0));
    }
}
