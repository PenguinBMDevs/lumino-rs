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
    ppqn: u32,
    /// 游标上一次推进到的 tick
    prev_tick: u64,
    /// 游标所在的 tempo 段索引（保证 `tempos[seg_idx].0 <= prev_tick`）
    seg_idx: usize,
    /// 游标位置（`prev_tick`）处的累计秒数
    secs_at_cursor: f64,
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
        // dedup_by_key 保留第一个，需反向处理以保留最后一个
        {
            let mut deduped: Vec<(u32, f32)> = Vec::with_capacity(tempos.len());
            for (t, bpm) in tempos.into_iter().rev() {
                if deduped.last().is_none_or(|(lt, _)| *lt != t) {
                    deduped.push((t, bpm));
                }
            }
            deduped.reverse();
            tempos = deduped;
        }
        // 确保至少有一个起始速度
        if !tempos.iter().any(|&(t, _)| t == 0) {
            tempos.insert(0, (0, 120.0));
        }

        Self {
            tempos,
            ppqn,
            prev_tick: 0,
            seg_idx: 0,
            secs_at_cursor: 0.0,
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
        let mut seconds = 0.0_f64;
        let mut current = 0_u64;

        for i in 0..self.tempos.len() {
            let seg_start = self.tempos[i].0 as u64;
            let seg_end = self
                .tempos
                .get(i + 1)
                .map(|t| t.0 as u64)
                .unwrap_or(u64::MAX);
            let bpm = self.tempos[i].1 as f64;

            if tick <= seg_start {
                break;
            }

            let seg_limit = tick.min(seg_end);
            let ticks = seg_limit - current;
            if ticks > 0 {
                seconds += ticks as f64 * 60.0 / (self.ppqn as f64 * bpm);
            }
            current = seg_limit;

            if current >= tick {
                break;
            }
        }

        seconds
    }

    /// 游标推进到 `tick`，返回从 tick 0 到该 tick 的累计秒数（O(1) 摊销）。
    ///
    /// 与 [`Self::tick_to_seconds`] 数值等价（同一累加公式），但利用"调用方 tick
    /// 非递减"的流式前提，只向前跨段、不回扫——旧实现每次 `advance_to` 都要从
    /// tempo 段 0 扫两遍，密集素材（百万级事件 × 数千 tempo 段）会退化为
    /// O(事件数 × 段数) 的纯扫描开销。
    ///
    /// `tick` 不大于游标位置时返回当前累计秒数（不后退、不更新游标）。
    pub fn seconds_at(&mut self, tick: u64) -> f64 {
        if tick <= self.prev_tick {
            return self.secs_at_cursor;
        }
        let mut secs = self.secs_at_cursor;
        let mut cur = self.prev_tick;
        while let Some(&(next_tick, _)) = self.tempos.get(self.seg_idx + 1) {
            let seg_end = next_tick as u64;
            if seg_end > tick {
                break;
            }
            if seg_end > cur {
                let bpm = self.tempos[self.seg_idx].1 as f64;
                secs += (seg_end - cur) as f64 * 60.0 / (self.ppqn as f64 * bpm);
                cur = seg_end;
            }
            self.seg_idx += 1;
        }
        if tick > cur {
            let bpm = self.tempos[self.seg_idx].1 as f64;
            secs += (tick - cur) as f64 * 60.0 / (self.ppqn as f64 * bpm);
        }
        self.prev_tick = tick;
        self.secs_at_cursor = secs;
        secs
    }

    /// 从上一次调用位置前进到 `tick`，返回 delta 秒数。
    ///
    /// 第一次调用相当于从 tick 0 到目标 tick 的秒数。
    /// tick 必须非递减（不会后退）。
    pub fn advance_to(&mut self, tick: u64) -> f64 {
        let before = self.secs_at_cursor;
        self.seconds_at(tick) - before
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
    fn test_1920ppq_constant() {
        // 120 BPM, PPQN=1920（编辑器默认 PPQ）→ 每秒 4 拍 = 3840 ticks/s。
        // 回归保护：渲染模块必须用文档真实 PPQ（doc.division）换算，
        // 此前硬编码 480 导致 1920 PPQ 文档 tick→秒放大 4 倍（时长/速度错误）。
        let tempos = vec![(0, 120.0)];
        let conv = TickToTime::new(tempos.clone(), 1920);
        let secs = conv.tick_to_seconds(3840);
        assert!(
            (secs - 1.0).abs() < 0.001,
            "120BPM 3840ticks = 1s, got {secs}"
        );
        // 同一 tick 数按 480 换算应是 4 秒（验证两者语义差异）
        let conv480 = TickToTime::new(tempos, 480);
        assert!(
            (conv480.tick_to_seconds(3840) - 4.0).abs() < 0.001,
            "同 tick 按 480 PPQ 换算应为 4s"
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

    /// 游标化后与逐点慢路径（`tick_to_seconds`）数值等价：
    /// 覆盖跨段、同 tick 去重、重复 tick、回退 tick 与大跳。
    #[test]
    fn cursor_matches_slow_path_across_segments() {
        let tempos = vec![
            (0, 120.0),
            (480, 60.0),
            (480, 90.0),
            (960, 45.0),
            (1920, 140.0),
        ];
        let mut conv = TickToTime::new(tempos.clone(), 480);
        let slow = TickToTime::new(tempos, 480);

        let ticks: [u64; 15] = [
            0, 1, 479, 480, 480, 481, 700, 959, 960, 961, 1919, 1920, 1921, 5000, 480,
        ];
        let mut cursor_abs = 0.0_f64;
        let mut prev = 0_u64;
        for &t in &ticks {
            let delta = conv.advance_to(t);
            cursor_abs += delta;
            if t > prev {
                // 绝对值与 delta 均应与慢路径一致
                assert!(
                    (cursor_abs - slow.tick_to_seconds(t)).abs() < 1e-9,
                    "tick={t} cursor_abs={cursor_abs} slow={}",
                    slow.tick_to_seconds(t)
                );
                let expected = slow.tick_to_seconds(t) - slow.tick_to_seconds(prev);
                assert!(
                    (delta - expected).abs() < 1e-9,
                    "tick={t} delta={delta} expected={expected}"
                );
                prev = t;
            } else {
                // 回退/重复 tick：游标单调不回退，delta 必须为 0 且不动游标
                assert_eq!(delta, 0.0, "回退 tick={t} 的 delta 应为 0");
            }
        }
        // 与 seconds_at（绝对查询）一致
        let mut conv2 = TickToTime::new(vec![(0, 120.0), (480, 60.0)], 480);
        assert!((conv2.seconds_at(960) - 1.5).abs() < 1e-9);
        assert!(
            (conv2.seconds_at(480) - 1.5).abs() < 1e-9,
            "回退查询应返回当前累计值"
        );
    }
}
