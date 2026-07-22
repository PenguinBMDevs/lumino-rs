//! 工程走带视图音符选择模型
//!
//! 移植自 yinhe 的 `Selection`：用一组矩形范围描述选中的音符，
//! 替代 HashSet<(track, tick, key)>，在 1000W 音符量级下内存极低。

/// 工程走带音符选择范围。
///
/// 每个矩形覆盖 `(tick_start, tick_end, key_lo, key_hi, track_lo, track_hi)`。
/// tick 区间为半开 `[tick_start, tick_end)`；track/key 区间为闭区间。
#[derive(Clone, Default, Debug)]
pub struct ArrangeSelection {
    /// 选择矩形列表。允许重叠，简单可依赖。
    pub rects: Vec<(u32, u32, u8, u8, u16, u16)>,
}

impl ArrangeSelection {
    /// 创建空选择。
    pub fn new() -> Self {
        Self::default()
    }

    /// 是否未选中任何音符。
    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }

    /// 清空选择。
    pub fn clear(&mut self) {
        self.rects.clear();
    }

    /// 添加一个通用矩形，默认覆盖全部 track。
    pub fn add_rect(&mut self, tick_start: u32, tick_end: u32, key_lo: u8, key_hi: u8) {
        self.add_rect_track(tick_start, tick_end, key_lo, key_hi, 0, u16::MAX);
    }

    /// 添加一个指定音轨范围的矩形。
    pub fn add_rect_track(
        &mut self,
        tick_start: u32,
        tick_end: u32,
        key_lo: u8,
        key_hi: u8,
        track_lo: u16,
        track_hi: u16,
    ) {
        if tick_end > tick_start {
            self.rects
                .push((tick_start, tick_end, key_lo, key_hi, track_lo, track_hi));
        }
    }

    /// 判断某个音符是否被选中。
    pub fn contains(&self, track: u16, start_tick: u32, key: u8) -> bool {
        self.rects.iter().any(|&(ts, te, kl, kh, tl, th)| {
            track >= tl
                && track <= th
                && key >= kl
                && key <= kh
                && start_tick >= ts
                && start_tick < te
        })
    }

    /// 矩形数量（用于估算快照大小）。
    pub fn len(&self) -> usize {
        self.rects.len()
    }

    /// 整体偏移 tick 与 key，key 限制在 [0, 127]，tick 限制 >= 0。
    pub fn offset(&mut self, delta_ticks: i64, delta_keys: i32) {
        for rect in &mut self.rects {
            let (ts, te, kl, kh, tl, th) = *rect;
            let new_ts = (ts as i64 + delta_ticks).max(0) as u32;
            let new_te = (te as i64 + delta_ticks).max(0) as u32;
            let new_kl = (kl as i32 + delta_keys).clamp(0, 127) as u8;
            let new_kh = (kh as i32 + delta_keys).clamp(0, 127) as u8;
            if new_te > new_ts {
                *rect = (new_ts, new_te, new_kl, new_kh, tl, th);
            }
        }
    }

    /// 仅偏移 tick 区间（工程走带拖拽用）。
    pub fn offset_ticks(&mut self, delta_ticks: i64) {
        for rect in &mut self.rects {
            let (ts, te, kl, kh, tl, th) = *rect;
            let new_ts = (ts as i64 + delta_ticks).max(0) as u32;
            let new_te = (te as i64 + delta_ticks).max(0) as u32;
            if new_te > new_ts {
                *rect = (new_ts, new_te, kl, kh, tl, th);
            }
        }
    }

    /// 仅偏移 track 区间（工程走带跨轨拖拽用）。
    pub fn offset_tracks(&mut self, delta_tracks: i32) {
        for rect in &mut self.rects {
            let (ts, te, kl, kh, tl, th) = *rect;
            let new_tl = (tl as i32 + delta_tracks).max(0) as u16;
            let new_th = (th as i32 + delta_tracks).max(0) as u16;
            *rect = (ts, te, kl, kh, new_tl, new_th);
        }
    }

    /// 计算与选择范围无关的哈希，用于 GPU 缓存键。
    pub fn hash(&self) -> u64 {
        let mut h: u64 = 0;
        for &(ts, te, kl, kh, tl, th) in &self.rects {
            h ^= (ts as u64).wrapping_mul(0x9e3779b97f4a7c15);
            h ^= (te as u64).wrapping_mul(0x9e3779b97f4a7c15);
            h ^= (kl as u64).wrapping_mul(0x9e3779b97f4a7c15);
            h ^= (kh as u64).wrapping_mul(0x9e3779b97f4a7c15);
            h ^= (tl as u64).wrapping_mul(0x9e3779b97f4a7c15);
            h ^= (th as u64).wrapping_mul(0x9e3779b97f4a7c15);
        }
        h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contains_and_offset() {
        let mut sel = ArrangeSelection::new();
        sel.add_rect_track(100, 200, 0, 127, 0, 2);
        assert!(sel.contains(1, 150, 60));
        assert!(!sel.contains(3, 150, 60));

        sel.offset_ticks(50);
        assert!(sel.contains(1, 210, 60));
        assert!(!sel.contains(1, 150, 60));

        sel.offset_tracks(1);
        assert!(sel.contains(2, 210, 60));
        assert!(!sel.contains(0, 210, 60));
    }
}
