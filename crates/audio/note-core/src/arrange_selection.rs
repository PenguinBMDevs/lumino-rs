//! 工程走带视图音符选择模型
//!
//! 移植自 yinhe 的 `Selection`：用一组矩形范围描述选中的音符，
//! 替代 HashSet<(track, tick, key)>，在 1000W 音符量级下内存极低。
//!
//! **冻结集（框选误伤修复）**：矩形是「时值区间」描述，一旦选择几何被
//! 变更（拖动平移 / 批量变速），矩形平移到新落点后会顺带覆盖落点区域内
//! **本来不在框选内**的既有音符——后续操作（再次拖动 / 删除 / 复制 /
//! 变速 / ghost 预览）会误伤它们。因此在几何变更点用 [`ArrangeSelection::freeze`]
//! 把「框选时内部包含的被框选音符」固定为精确集合：此后 `contains` 走精确
//! 成员判定，矩形降级为显示 / 命中 / 粘贴锚点用边界。

use std::collections::HashMap;

/// 冻结音符集：精确 `(视觉音轨, start_tick, key)` 成员判定。
///
/// 紧凑表示：每个视觉音轨一条升序 `u64` 序列，元素为
/// `((start_tick as u64) << 8) | key`（8 字节/音符，二分查找 O(log K)）。
#[derive(Clone, Default, Debug)]
pub struct FrozenNotes {
    /// 视觉音轨 → 升序打包键
    by_track: HashMap<u16, Vec<u64>>,
    /// 冻结音符总数（按 (start_tick, key) 去重后）
    len: usize,
}

/// `(start_tick, key)` 打包为可排序的 `u64`
#[inline]
fn pack(start_tick: u32, key: u8) -> u64 {
    ((start_tick as u64) << 8) | key as u64
}

impl FrozenNotes {
    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// 冻结音符数（去重后）
    pub fn len(&self) -> usize {
        self.len
    }

    /// 清空
    pub fn clear(&mut self) {
        self.by_track.clear();
        self.len = 0;
    }

    /// 由 `(视觉音轨, start_tick, key)` 迭代器构建（去重 + 按音轨升序）
    pub fn from_entries<I: IntoIterator<Item = (u16, u32, u8)>>(entries: I) -> Self {
        let mut by_track: HashMap<u16, Vec<u64>> = HashMap::new();
        for (track, start_tick, key) in entries {
            by_track
                .entry(track)
                .or_default()
                .push(pack(start_tick, key));
        }
        let mut len = 0usize;
        for keys in by_track.values_mut() {
            keys.sort_unstable();
            keys.dedup();
            len += keys.len();
        }
        Self { by_track, len }
    }

    /// 精确成员判定（按打包键二分，O(log K)）
    pub fn contains(&self, track: u16, start_tick: u32, key: u8) -> bool {
        let Some(keys) = self.by_track.get(&track) else {
            return false;
        };
        keys.binary_search(&pack(start_tick, key)).is_ok()
    }

    /// 按变换函数重建（整体偏移等几何变更用，闭包返回新 `(音轨, start_tick, key)`）
    pub fn transform<F>(&mut self, mut f: F)
    where
        F: FnMut(u16, u32, u8) -> (u16, u32, u8),
    {
        let mut by_track: HashMap<u16, Vec<u64>> = HashMap::with_capacity(self.by_track.len());
        for (&track, keys) in &self.by_track {
            for &packed in keys {
                let (nt, ns, nk) = f(track, (packed >> 8) as u32, packed as u8);
                by_track.entry(nt).or_default().push(pack(ns, nk));
            }
        }
        let mut len = 0usize;
        for keys in by_track.values_mut() {
            keys.sort_unstable();
            keys.dedup();
            len += keys.len();
        }
        self.by_track = by_track;
        self.len = len;
    }
}

/// 工程走带音符选择范围。
///
/// 每个矩形覆盖 `(tick_start, tick_end, key_lo, key_hi, track_lo, track_hi)`。
/// tick 区间为半开 `[tick_start, tick_end)`；track/key 区间为闭区间。
///
/// `frozen` 为 `Some` 时（拖动 / 变速后的落点重锚），`contains` 以冻结集
/// **精确成员判定**为准，`rects` 仅保留显示 / 命中 / 锚点语义。
#[derive(Clone, Default, Debug)]
pub struct ArrangeSelection {
    /// 选择矩形列表。允许重叠，简单可依赖。
    pub rects: Vec<(u32, u32, u8, u8, u16, u16)>,
    /// 冻结音符集（几何变更后的精确选择集，见模块文档）
    frozen: Option<FrozenNotes>,
}

impl ArrangeSelection {
    /// 创建空选择。
    pub fn new() -> Self {
        Self::default()
    }

    /// 是否未选中任何音符。
    pub fn is_empty(&self) -> bool {
        self.rects.is_empty() && self.frozen.as_ref().is_none_or(FrozenNotes::is_empty)
    }

    /// 清空选择。
    pub fn clear(&mut self) {
        self.rects.clear();
        self.frozen = None;
    }

    /// 冻结音符集（几何变更后的精确选择集），无冻结时为 `None`
    pub fn frozen(&self) -> Option<&FrozenNotes> {
        self.frozen.as_ref()
    }

    /// 冻结为「框选时内部包含的被框选音符」的精确集合（拖动 / 批量变速后调用）。
    ///
    /// 入参 `notes` 为**变更后**的实际位置 `(视觉音轨, start_tick, end_tick, key)`，
    /// 应为文档权威 tick（经 `f32_to_tick` 转换后的值），保证 `contains` 能被
    /// 文档遍历逐个命中。
    ///
    /// 冻结后：
    /// - `contains` → 精确成员判定（落点区域内既有的其他音符不再被误伤）；
    /// - `rects` → 整块紧致边界（四项 bbox 并集，仅作显示 / 命中 / 锚点用）。
    pub fn freeze<I>(&mut self, notes: I)
    where
        I: IntoIterator<Item = (u16, u32, u32, u8)>,
    {
        let mut entries: Vec<(u16, u32, u8)> = Vec::new();
        let mut bbox: Option<(u32, u32, u8, u8, u16, u16)> = None;
        for (track, start_tick, end_tick, key) in notes {
            entries.push((track, start_tick, key));
            let end = end_tick.max(start_tick);
            bbox = Some(match bbox {
                Some((ts, te, kl, kh, tl, th)) => (
                    ts.min(start_tick),
                    te.max(end),
                    kl.min(key),
                    kh.max(key),
                    tl.min(track),
                    th.max(track),
                ),
                None => (start_tick, end, key, key, track, track),
            });
        }
        // 去重 + 按音轨升序（构建期一次性排序，查询 O(log K)）
        let frozen = FrozenNotes::from_entries(entries);
        self.frozen = if frozen.is_empty() {
            None
        } else {
            Some(frozen)
        };
        self.rects.clear();
        if let Some((ts, te, kl, kh, tl, th)) = bbox {
            // 半开区间 `[ts, te)`：至少覆盖一个 tick，保持 contains 语义可用
            self.rects.push((ts, te.max(ts + 1), kl, kh, tl, th));
        }
    }

    /// 添加一个通用矩形，默认覆盖全部 track。
    pub fn add_rect(&mut self, tick_start: u32, tick_end: u32, key_lo: u8, key_hi: u8) {
        self.add_rect_track(tick_start, tick_end, key_lo, key_hi, 0, u16::MAX);
    }

    /// 添加一个指定音轨范围的矩形。
    ///
    /// 冻结集与矩形不可混用：存在冻结集时先丢弃冻结集**及其派生的紧致边界**
    /// （避免派生的 bbox 矩形在冻结集被丢弃后继续按矩形语义命中落点音符），
    /// 再添加新矩形；无冻结集时保持「追加矩形」语义（重叠矩形取并集）。
    pub fn add_rect_track(
        &mut self,
        tick_start: u32,
        tick_end: u32,
        key_lo: u8,
        key_hi: u8,
        track_lo: u16,
        track_hi: u16,
    ) {
        if self.frozen.take().is_some() {
            self.rects.clear();
        }
        if tick_end > tick_start {
            self.rects
                .push((tick_start, tick_end, key_lo, key_hi, track_lo, track_hi));
        }
    }

    /// 判断某个音符是否被选中。
    ///
    /// 冻结集存在时：先按紧致边界 O(1) 排除（全曲遍历场景绝大多数音符在此滤掉），
    /// 命中边界内再做精确二分成员判定（拖动 / 变速后的落点重锚）。
    /// 无冻结集时走纯矩形判定。
    pub fn contains(&self, track: u16, start_tick: u32, key: u8) -> bool {
        if let Some(frozen) = &self.frozen {
            return self.within_bounds(track, start_tick, key)
                && frozen.contains(track, start_tick, key);
        }
        self.rects_contain(track, start_tick, key)
    }

    /// 矩形判定（`[tick_start, tick_end)` 半开区间语义，纯矩形选择用）
    #[inline]
    fn rects_contain(&self, track: u16, start_tick: u32, key: u8) -> bool {
        self.rects.iter().any(|&(ts, te, kl, kh, tl, th)| {
            track >= tl
                && track <= th
                && key >= kl
                && key <= kh
                && start_tick >= ts
                && start_tick < te
        })
    }

    /// 紧致边界快速排除（仅冻结集路径用）
    ///
    /// 上界**含等号**：边界由冻结音符的 `max_end` 给出，端点音符（零长 / 尾部）
    /// 的 `start_tick` 恰等于 `te`，半开区间会把合法冻结音符滤掉。
    /// 该判定只做「超集过滤」，精确成员与否仍由冻结集二分决定。
    #[inline]
    fn within_bounds(&self, track: u16, start_tick: u32, key: u8) -> bool {
        self.rects.iter().any(|&(ts, te, kl, kh, tl, th)| {
            track >= tl
                && track <= th
                && key >= kl
                && key <= kh
                && start_tick >= ts
                && start_tick <= te
        })
    }

    /// 矩形数量（用于估算快照大小）。
    pub fn len(&self) -> usize {
        self.rects.len()
    }

    /// 整体偏移 tick 与 key，key 限制在 [0, 127]，tick 限制 >= 0。
    ///
    /// 冻结集存在时同步偏移（保持精确集合与文档一致）。
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
        if let Some(frozen) = &mut self.frozen {
            frozen.transform(|track, start_tick, key| {
                (
                    track,
                    (start_tick as i64 + delta_ticks).max(0) as u32,
                    (key as i32 + delta_keys).clamp(0, 127) as u8,
                )
            });
        }
    }

    /// 仅偏移 tick 区间（工程走带拖拽用）。冻结集同步偏移（tick 下限 0）。
    pub fn offset_ticks(&mut self, delta_ticks: i64) {
        for rect in &mut self.rects {
            let (ts, te, kl, kh, tl, th) = *rect;
            let new_ts = (ts as i64 + delta_ticks).max(0) as u32;
            let new_te = (te as i64 + delta_ticks).max(0) as u32;
            if new_te > new_ts {
                *rect = (new_ts, new_te, kl, kh, tl, th);
            }
        }
        if let Some(frozen) = &mut self.frozen {
            frozen.transform(|track, start_tick, key| {
                (track, (start_tick as i64 + delta_ticks).max(0) as u32, key)
            });
        }
    }

    /// 仅偏移 track 区间（工程走带跨轨拖拽用）。冻结集同步偏移（音轨下限 0）。
    pub fn offset_tracks(&mut self, delta_tracks: i32) {
        for rect in &mut self.rects {
            let (ts, te, kl, kh, tl, th) = *rect;
            let new_tl = (tl as i32 + delta_tracks).max(0) as u16;
            let new_th = (th as i32 + delta_tracks).max(0) as u16;
            *rect = (ts, te, kl, kh, new_tl, new_th);
        }
        if let Some(frozen) = &mut self.frozen {
            frozen.transform(|track, start_tick, key| {
                ((track as i32 + delta_tracks).max(0) as u16, start_tick, key)
            });
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
        assert!(sel.contains(1, 150, 60));

        sel.offset_tracks(1);
        assert!(sel.contains(2, 210, 60));
        assert!(!sel.contains(0, 210, 60));
    }

    #[test]
    fn test_freeze_contains_is_exact() {
        // 冻结为两个精确音符：矩形平移覆盖不到的落点音符不再被误伤
        let mut sel = ArrangeSelection::new();
        sel.freeze([(0u16, 300u32, 400u32, 60u8), (0, 500, 600, 64)]);

        assert!(sel.contains(0, 300, 60), "冻结音符应命中");
        assert!(sel.contains(0, 500, 64), "冻结音符应命中");
        // 同一矩形区间内的其他 (tick, key) 不得命中（框选误伤修复核心）
        assert!(!sel.contains(0, 300, 64), "同 tick 不同 key 不应命中");
        assert!(!sel.contains(0, 400, 60), "同 key 不同 tick 不应命中");
        assert!(!sel.contains(1, 300, 60), "不同音轨不应命中");
        assert_eq!(sel.frozen().map(FrozenNotes::len), Some(2));
        assert!(!sel.is_empty());
    }

    #[test]
    fn test_freeze_bbox_covers_notes_only() {
        // 冻结后矩形为紧致边界（显示/命中用），四项并集
        let mut sel = ArrangeSelection::new();
        sel.freeze([(2u16, 100u32, 200u32, 60u8), (2, 400, 500, 72)]);
        let (ts, te, kl, kh, tl, th) = sel.rects[0];
        assert_eq!((ts, te, kl, kh, tl, th), (100, 500, 60, 72, 2, 2));
    }

    #[test]
    fn test_freeze_offset_keeps_exact_set_in_sync() {
        let mut sel = ArrangeSelection::new();
        sel.freeze([(0u16, 300u32, 400u32, 60u8)]);
        sel.offset_ticks(100);
        sel.offset_tracks(1);
        assert!(sel.contains(1, 400, 60), "冻结集应随偏移同步");
        assert!(!sel.contains(0, 400, 60));
        assert!(!sel.contains(1, 300, 60));
    }

    #[test]
    fn test_add_rect_drops_frozen_set() {
        // 新框选（添加矩形）回到矩形判定语义
        let mut sel = ArrangeSelection::new();
        sel.freeze([(0u16, 300u32, 400u32, 60u8)]);
        sel.add_rect_track(100, 200, 0, 127, 0, 0);
        assert!(sel.frozen().is_none());
        assert!(sel.contains(0, 150, 60));
        assert!(!sel.contains(0, 300, 60), "冻结集已被新矩形选择取代");
    }

    #[test]
    fn test_clear_resets_frozen_set() {
        let mut sel = ArrangeSelection::new();
        sel.freeze([(0u16, 300u32, 400u32, 60u8)]);
        sel.clear();
        assert!(sel.is_empty());
        assert!(sel.frozen().is_none());
        assert!(!sel.contains(0, 300, 60));
    }
}
