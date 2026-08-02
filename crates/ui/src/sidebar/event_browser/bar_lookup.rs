//! Tick → 拍号位置格式化（"小节/拍内 tick"）。
//!
//! 支持变拍号：根据 `time_signatures` 的变化分段计算。

pub(super) struct BarLookup {
    segs: Vec<BarSeg>,
}

struct BarSeg {
    tick_start: u32,
    bar_start: u32,
    ticks_per_bar: u32,
}

impl BarLookup {
    /// 从 PPQ、默认拍号分子和拍号变化点构建小节查找表。
    ///
    /// `ts_changes` 中每一项为 `(tick, numerator)`。若首项 tick 不为 0，
    /// 则自动插入默认拍号 `(0, default_num)`。
    pub(super) fn build(ppq: u32, default_num: u8, ts_changes: &[(u32, u8)]) -> Self {
        let mut points: Vec<(u32, u8)> = Vec::new();
        if ts_changes.first().map(|e| e.0).unwrap_or(u32::MAX) != 0 {
            points.push((0, default_num));
        }
        for &(tick, num) in ts_changes {
            points.push((tick, num));
        }

        let mut segs = Vec::with_capacity(points.len());
        let mut cum_bars: u32 = 0;
        for (i, &(tick, num)) in points.iter().enumerate() {
            let ticks_per_bar = ppq.saturating_mul(num.max(1) as u32);
            segs.push(BarSeg {
                tick_start: tick,
                bar_start: cum_bars,
                ticks_per_bar,
            });
            if let Some(&(next_tick, _)) = points.get(i + 1) {
                let span = next_tick.saturating_sub(tick);
                cum_bars = cum_bars.saturating_add(span / ticks_per_bar.max(1));
            }
        }

        if segs.is_empty() {
            segs.push(BarSeg {
                tick_start: 0,
                bar_start: 0,
                ticks_per_bar: ppq.saturating_mul(4),
            });
        }

        BarLookup { segs }
    }

    /// 将 tick 格式化为 `小节/小节内 tick`。
    pub(super) fn format(&self, tick: u32) -> String {
        let (bar, tick_in_bar) = self.tick_to_position(tick);
        format!("{}/{}", bar, tick_in_bar)
    }

    /// tick → (小节号, 小节内 tick)。小节号从 1 开始。
    pub(super) fn tick_to_position(&self, tick: u32) -> (u32, u32) {
        if self.segs.is_empty() {
            return (1, 0);
        }
        let idx = match self.segs.binary_search_by_key(&tick, |s| s.tick_start) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let seg = &self.segs[idx];
        let local = tick.saturating_sub(seg.tick_start);
        let tpb = seg.ticks_per_bar.max(1);
        let bar_offset = local / tpb;
        let tick_in_bar = local % tpb;
        let bar_1 = seg.bar_start + bar_offset + 1;
        (bar_1, tick_in_bar)
    }

    /// (小节号, 小节内 tick) → tick。小节号从 1 开始。
    ///
    /// `bar` < 1 视为 1。`tick_in_bar` 允许溢出（用户自由输入）。
    #[allow(dead_code)] // 预留：popup 位置编辑回写
    pub(super) fn position_to_tick(&self, bar: u32, tick_in_bar: u32) -> u32 {
        if self.segs.is_empty() {
            return tick_in_bar;
        }
        let target_bar_0 = (bar.max(1) as i64 - 1).max(0); // 0-based
        // 找 target_bar_0 所在的 segment：最后一个 bar_start <= target_bar_0 的 seg
        let seg = self
            .segs
            .iter()
            .take_while(|s| s.bar_start as i64 <= target_bar_0)
            .last()
            .unwrap_or(&self.segs[0]);
        let bar_offset = target_bar_0 - seg.bar_start as i64;
        let tick = seg.tick_start as i64
            + bar_offset * seg.ticks_per_bar.max(1) as i64
            + tick_in_bar as i64;
        tick.max(0) as u32
    }
}

/// 从完整的 `(tick, numerator, denominator)` 拍号表提取 `BarLookup::build` 所需格式。
pub(super) fn ts_changes(time_sigs: &[(u32, u8, u8)]) -> Vec<(u32, u8)> {
    time_sigs
        .iter()
        .map(|(tick, num, _)| (*tick, *num))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_lookup_single_segment() {
        let bl = BarLookup::build(480, 4, &[]);
        assert_eq!(bl.format(0), "1/0");
        assert_eq!(bl.format(480), "1/480");
        assert_eq!(bl.format(1920), "2/0");
    }

    #[test]
    fn bar_lookup_with_time_sig() {
        let bl = BarLookup::build(480, 4, &[(0, 4)]);
        assert_eq!(bl.format(0), "1/0");
        assert_eq!(bl.format(480), "1/480");
    }

    #[test]
    fn bar_lookup_time_sig_change() {
        let bl = BarLookup::build(480, 4, &[(0, 4), (1920, 3)]);
        assert_eq!(bl.format(0), "1/0");
        assert_eq!(bl.format(1920), "2/0");
        assert_eq!(bl.format(2400), "2/480");
    }

    #[test]
    fn bar_lookup_format_tick_zero() {
        let bl = BarLookup::build(480, 4, &[]);
        assert_eq!(bl.format(0), "1/0");
    }

    #[test]
    fn bar_lookup_default_time_sig() {
        let bl = BarLookup::build(480, 4, &[]);
        assert_eq!(bl.format(960), "1/960");
    }

    #[test]
    fn bar_lookup_format_bar_start() {
        let bl = BarLookup::build(480, 4, &[]);
        assert_eq!(bl.format(1920), "2/0");
        assert_eq!(bl.format(3840), "3/0");
    }

    #[test]
    fn bar_lookup_first_ts_after_zero_uses_default() {
        let bl = BarLookup::build(480, 4, &[(1920, 3)]);
        assert_eq!(bl.format(0), "1/0");
        assert_eq!(bl.format(1920), "2/0");
    }

    #[test]
    fn bar_lookup_position_roundtrip() {
        let bl = BarLookup::build(480, 4, &[(0, 4), (1920, 3)]);
        // 正向 tick → position 再反向 position → tick
        for tick in [0, 480, 960, 1920, 2400, 3360] {
            let (bar, tib) = bl.tick_to_position(tick);
            assert_eq!(
                bl.position_to_tick(bar, tib),
                tick,
                "tick {} -> bar/tick {}/{} should round-trip",
                tick,
                bar,
                tib
            );
        }
    }

    #[test]
    fn bar_lookup_ts_changes_extracts_numerator_only() {
        let full = vec![(0, 4, 4), (1920, 3, 8), (3840, 5, 4)];
        assert_eq!(ts_changes(&full), vec![(0, 4), (1920, 3), (3840, 5)]);
    }
}
