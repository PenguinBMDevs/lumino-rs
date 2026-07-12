//! Tick→时间转换器 — 将 MIDI tick 转换为秒（含速度变化处理）

/// 将 MIDI tick 流转换为秒的转换器。
///
/// 根据 tempo 变化表（(tick, bpm)，按 tick 升序）逐段计算。
/// 每次调用 `advance_to(tick)` 返回从上一次位置到当前 tick 的 delta 秒数。
///
/// # 用法
///
/// ```ignore
/// let mut conv = TickToTime::new(tempos, ppqn);
/// let delta = conv.advance_to(480);  // 从 tick 0 -> 480 需要多少秒
/// ```
pub struct TickToTime {
    /// (tick, bpm) 按 tick 升序排列
    tempos: Vec<(u32, f32)>,
    /// 预计算的累积秒数：cum_secs[i] = 从 tick 0 到 tempos[i].tick 的总秒数
    cum_secs: Vec<f64>,
    ppqn: u32,
    /// 上一次调用 advance_to 时的 tick
    prev_tick: u64,
}

impl TickToTime {
    /// 创建新的转换器。
    ///
    /// # 参数
    ///
    /// - `tempos`: 速度变化列表 (tick, bpm)，按 tick 升序。**必须**包含至少一个起始速度。
    /// - `ppqn`: MIDI 文件的 PPQN（每四分音符脉冲数）
    pub fn new(mut tempos: Vec<(u32, f32)>, ppqn: u32) -> Self {
        // 确保按 tick 排序
        tempos.sort_by_key(|&(t, _)| t);
        // 去重：同 tick 的保留最后一个（后加载的覆盖前面的）
        tempos.dedup_by_key(|&mut (t, _)| t);
        // 确保至少有一个起始速度
        if !tempos.iter().any(|&(t, _)| t == 0) {
            tempos.insert(0, (0, 120.0));
        }

        // 预计算每个 tempo 边界处的累积秒数
        let mut cum_secs = Vec::with_capacity(tempos.len());
        let mut seconds_acc = 0.0_f64;
        let ppqn_f = ppqn as f64;
        for i in 0..tempos.len() {
            if i > 0 {
                let prev_tick = tempos[i - 1].0 as u64;
                let cur_tick = tempos[i].0 as u64;
                let ticks = cur_tick - prev_tick;
                let bpm = tempos[i - 1].1 as f64;
                seconds_acc += ticks as f64 * 60.0 / (ppqn_f * bpm);
            }
            cum_secs.push(seconds_acc);
        }

        Self {
            tempos,
            cum_secs,
            ppqn,
            prev_tick: 0,
        }
    }

    /// 将指定 tick 转换为从起始位置到该 tick 的总秒数。
    ///
    /// # 参数
    ///
    /// - `tick`: 目标 tick
    ///
    /// # 返回
    ///
    /// 从 tick 0 到目标 tick 的总秒数
    pub fn tick_to_seconds(&self, tick: u64) -> f64 {
        // 二分查找：找到 tick 所在的 tempo 段
        // 找到最后一个 cum_secs[i].tick <= tick 的 i
        let idx = self.tempos.partition_point(|&(t, _)| (t as u64) <= tick);
        // idx 是第一个 tick > target 的位置，所以 idx-1 是最后一个 <= target 的

        if idx == 0 {
            // tick 在第一个 tempo 之前（不应该发生，因为第一个 tick=0）
            return 0.0;
        }

        let seg_idx = idx - 1;
        let base_secs = self.cum_secs[seg_idx];
        let seg_start_tick = self.tempos[seg_idx].0 as u64;
        let ticks = tick - seg_start_tick;
        let bpm = self.tempos[seg_idx].1 as f64;

        base_secs + ticks as f64 * 60.0 / (self.ppqn as f64 * bpm)
    }

    /// 从上一次调用位置前进到 `tick`，返回 delta 秒数。
    ///
    /// 第一次调用相当于从 tick 0 到目标 tick 的秒数。
    /// tick 必须非递减（不会后退）。
    pub fn advance_to(&mut self, tick: u64) -> f64 {
        let cur = self.prev_tick;
        if tick <= cur {
            return 0.0;
        }

        // 直接用 tick_to_seconds 计算差值
        let total_at_cur = self.tick_to_seconds(cur);
        let total_at_tick = self.tick_to_seconds(tick);
        self.prev_tick = tick;
        total_at_tick - total_at_cur
    }

    /// 重置转换器到 tick 0
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.prev_tick = 0;
    }

    /// 返回总 MIDI 时长（秒）
    #[allow(dead_code)]
    pub fn total_seconds(&self, total_ticks: u64) -> f64 {
        self.tick_to_seconds(total_ticks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_120bpm_constant() {
        // 120 BPM, PPQN=480 → 每秒 2 拍 = 960 ticks/s
        let tempos = vec![(0, 120.0)];
        let conv = TickToTime::new(tempos, 480);
        let secs = conv.tick_to_seconds(960);
        assert!(
            (secs - 1.0).abs() < 0.001,
            "120BPM 960ticks = 1s, got {secs}"
        );
    }

    #[test]
    fn test_tempo_change() {
        // tick 0: 120 BPM, tick 480: 60 BPM
        // 0-480 ticks @120BPM = 0.5s
        // 480-960 ticks @60BPM = 1.0s
        // total = 1.5s
        let tempos = vec![(0, 120.0), (480, 60.0)];
        let conv = TickToTime::new(tempos, 480);
        let secs = conv.tick_to_seconds(960);
        assert!((secs - 1.5).abs() < 0.001, "960ticks = 1.5s, got {secs}");
    }

    #[test]
    fn test_advance_to_progressive() {
        let mut conv = TickToTime::new(vec![(0, 120.0)], 480);
        let d1 = conv.advance_to(480); // 0.5s
        assert!((d1 - 0.5).abs() < 0.001, "first delta = 0.5s, got {d1}");
        let d2 = conv.advance_to(960); // 0.5s
        assert!((d2 - 0.5).abs() < 0.001, "second delta = 0.5s, got {d2}");
    }
}
