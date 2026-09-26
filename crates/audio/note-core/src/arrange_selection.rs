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

mod frozen;
#[cfg(test)]
mod tests;

pub use frozen::FrozenNotes;

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
    ///
    /// **只读消费方**请走 `contains` / `len` / `hash` / `revision`；
    /// 写入必须走本类型的方法（它们会 bump [`Self::revision`]），
    /// 否则依赖 revision 的派生缓存（选区命中音符）会读到脏数据。
    pub rects: Vec<(u32, u32, u8, u8, u16, u16)>,
    /// 冻结音符集（几何变更后的精确选择集，见模块文档）
    frozen: Option<FrozenNotes>,
    /// 变更版本号：任一变更入口（`clear` / `freeze` / `add_rect_track` /
    /// `offset*`）自增，供派生缓存判失效。
    ///
    /// 2026-09 性能修复：走带选区的派生数据（命中音符列表）此前在 view 层
    /// **每帧全量扫描全文档音符**（框选后 12.9ms/帧）。该派生值是
    /// `(document, 本选区)` 的纯函数，引入版本号后只需在选区真正变化时重算。
    revision: u64,
}

impl ArrangeSelection {
    /// 创建空选择。
    pub fn new() -> Self {
        Self::default()
    }

    /// 变更版本号（选区任一变更即自增）。
    ///
    /// 与 `EditorData::track_notes_gen` 组合构成派生缓存的失效键：
    /// 前者覆盖「音符变了」，本值覆盖「选区变了」，两者缺一不可——
    /// 文档不变而选区平移（`offset_ticks`）同样会让命中集合改变。
    #[inline]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// bump 变更版本号（内部使用）。
    #[inline]
    fn bump_revision(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    /// 是否未选中任何音符。
    pub fn is_empty(&self) -> bool {
        self.rects.is_empty() && self.frozen.as_ref().is_none_or(FrozenNotes::is_empty)
    }

    /// 清空选择。
    pub fn clear(&mut self) {
        self.rects.clear();
        self.frozen = None;
        self.bump_revision();
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
        self.bump_revision();
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
        self.bump_revision();
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
                    (track as i32 + delta_keys).clamp(0, 127) as u16,
                    (start_tick as i64 + delta_ticks).max(0) as u32,
                    (key as i32 + delta_keys).clamp(0, 127) as u8,
                )
            });
        }
        self.bump_revision();
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
        self.bump_revision();
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
        self.bump_revision();
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
