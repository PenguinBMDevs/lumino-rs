//! 洋葱皮缓存基础设施
//!
//! 包含缓存数据结构、合并函数、哈希计算和 output 重建。
//! 与 Editor 自身状态无关，纯数据层。

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use lumino_gfx::{NoteInstance, pack_color};

/// 专用 rayon 线程池，避免与 UI/iced 的全局线程池竞争。
/// 只用于洋葱皮音轨的并行查询+合并。
pub(super) static ONION_SKIN_POOL: LazyLock<rayon::ThreadPool> = LazyLock::new(|| {
    rayon::ThreadPoolBuilder::new()
        .thread_name(|i| format!("onion-skin-{}", i))
        .build()
        .expect("Failed to create onion skin thread pool")
});

/// 洋葱皮结果缓存：记录上一帧的合并结果和视口。
/// 滚动的帧只做增量更新（补新进视口的 cell），不做 799 轨全量重查。
pub(super) struct OnionSkinCache {
    /// 量化值
    pub tick_quant: u32,
    /// 视口范围
    pub search_start: f32,
    pub search_end: f32,
    pub search_key_min: u16,
    pub search_key_max: u16,
    /// 音轨配置哈希（仅可见音轨索引，不含颜色）
    pub track_hash: u64,
    /// 音轨配置哈希（可见音轨 + 颜色，用于全量失效检测）
    pub config_hash: u64,
    /// 合并后的格子（color 字段已解耦，仅存 track_idx）
    pub cells: HashMap<u64, MergedCell>,
    /// 输出缓存 — Arc 引用，clone 仅原子操作，无深拷贝
    pub output: std::sync::Arc<Vec<NoteInstance>>,
    /// 标记仅颜色/透明度变化，无需重查 document
    pub colors_dirty: bool,
    /// 脏音轨集合：需要增量重查的音轨索引
    /// 实现分轨失效：单轨修改时，只移除该轨贡献的 cells、重新查询、合并
    pub dirty_tracks: HashSet<u16>,
}

pub(super) static ONION_SKIN_CACHE: LazyLock<std::sync::RwLock<Option<OnionSkinCache>>> =
    LazyLock::new(|| std::sync::RwLock::new(None));

/// 合并中的格子状态：在同一格子内的音符合并成一个大矩形
/// 使用 tick_start == EMPTY 标记空格子，规避 Option 对齐开销
///
/// 优化：将 color: [f32; 4] 替换为 track_idx: u16
/// 1. 格子大小从 28→14 字节，减半
/// 2. 解耦数据与视觉：颜色/透明度变化时无需重查 document
///    只需从 cells 重建 output，走 O(C) 快速路径
#[derive(Copy, Clone)]
pub(super) struct MergedCell {
    pub tick_start: f32,
    pub max_right: f32,
    pub key: f32,
    pub track_idx: u16,
}

impl MergedCell {
    #[allow(dead_code)]
    const EMPTY: f32 = f32::MAX;

    #[allow(dead_code)]
    pub(super) const fn empty() -> Self {
        Self {
            tick_start: Self::EMPTY,
            max_right: 0.0,
            key: 0.0,
            track_idx: 0,
        }
    }

    #[allow(dead_code)]
    pub(super) fn is_empty(&self) -> bool {
        self.tick_start == Self::EMPTY
    }
}

/// 将单个音轨的原始音符合并到 HashMap
pub(super) fn merge_one_track(
    raw: &[(f32, u8, f32, u8, u8)],
    search_start: f32,
    search_end: f32,
    search_key_min: u16,
    search_key_max: u16,
    tick_quant: u32,
    track_idx: u16,
) -> Option<HashMap<u64, MergedCell>> {
    let mut cells = HashMap::new();

    for &(tick, key, length, _vel, _ch) in raw.iter() {
        let key_u16 = key as u16;
        if key_u16 >= search_key_min
            && key_u16 <= search_key_max
            && tick + length >= search_start
            && tick <= search_end
        {
            let cell_x = (tick as u32 / tick_quant) as u64;
            let cell_key = (cell_x << 32) | (key_u16 as u64);
            let right = tick + length;

            cells
                .entry(cell_key)
                .and_modify(|m: &mut MergedCell| {
                    if tick < m.tick_start {
                        m.tick_start = tick;
                    }
                    if right > m.max_right {
                        m.max_right = right;
                    }
                    // 后入者覆盖：小 track_idx 在上层（渲染后绘制，覆盖大 track_idx）
                    // 降序排列保证：大 idx 先入底层，小 idx 后入顶层
                    m.track_idx = track_idx;
                })
                .or_insert(MergedCell {
                    tick_start: tick,
                    max_right: right,
                    key: key as f32,
                    track_idx,
                });
        }
    }

    if cells.is_empty() { None } else { Some(cells) }
}

/// 将 cell 合并到已有的 HashMap（后入的为上层，覆盖 track_idx）
pub(super) fn merge_cell(acc: &mut HashMap<u64, MergedCell>, key: u64, cell: MergedCell) {
    match acc.entry(key) {
        std::collections::hash_map::Entry::Occupied(mut e) => {
            let m = e.get_mut();
            if cell.tick_start < m.tick_start {
                m.tick_start = cell.tick_start;
            }
            if cell.max_right > m.max_right {
                m.max_right = cell.max_right;
            }
            m.track_idx = cell.track_idx;
        }
        std::collections::hash_map::Entry::Vacant(e) => {
            e.insert(cell);
        }
    }
}

impl OnionSkinCache {
    /// 检查是否可以增量更新（仅水平滚动方向增量）
    ///
    /// 优化记录：
    /// - 放宽 key 范围限制：纵向滚动不再触发全量重建，
    ///   因为 key 范围变化只影响合并时的过滤，不影响已有 cell 的正确性。
    ///   新 key 范围的增量查询会在主函数中处理。
    /// - 分离 track_hash（仅含音轨索引）和 config_hash（含颜色）：
    ///   颜色变化时 track_hash 不变，可走 colors_dirty 快速路径。
    pub(super) fn can_incremental(
        &self,
        tick_quant: u32,
        search_start: f32,
        search_end: f32,
        search_key_min: u16,
        search_key_max: u16,
        track_hash: u64,
    ) -> bool {
        // 音轨集合变了 → 不可增量（必须全量重建）
        if self.track_hash != track_hash {
            tracing::info!("CACHE MISS: track_hash changed");
            return false;
        }
        // 量化变了（缩放） → 不可增量
        if self.tick_quant != tick_quant {
            tracing::info!(
                "CACHE MISS: tick_quant {} != {}",
                self.tick_quant,
                tick_quant
            );
            return false;
        }
        // 视口完全没变 → 直接命中缓存
        if self.search_start == search_start
            && self.search_end == search_end
            && self.search_key_min == search_key_min
            && self.search_key_max == search_key_max
        {
            return true;
        }
        // 水平视口偏移太大（> 30% 缓存宽度） → 全量重查更快
        let cache_span = self.search_end - self.search_start;
        if cache_span <= 0.0 {
            return false;
        }
        let shift = (search_start - self.search_start).abs();
        let is_nearby = shift < cache_span * 0.3;
        if !is_nearby {
            tracing::info!("CACHE MISS: shift={} > 30% of span={}", shift, cache_span,);
        }
        is_nearby
    }
}

/// 计算音轨配置哈希（包含颜色，用于全量失效检测）
pub(super) fn track_config_hash(track_colors: &[(usize, [f32; 4])]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    track_colors.len().hash(&mut h);
    for &(idx, color) in track_colors {
        idx.hash(&mut h);
        color[0].to_bits().hash(&mut h);
        color[1].to_bits().hash(&mut h);
        color[2].to_bits().hash(&mut h);
        color[3].to_bits().hash(&mut h);
    }
    h.finish()
}

/// 计算音轨索引哈希（不含颜色，用于增量路径检测）
///
/// 颜色变化不影响增量可行性——只要音轨集合不变，
/// 已有的 cells 数据仍然正确，只需重建 output。
pub(super) fn track_hash_no_color(track_colors: &[(usize, [f32; 4])]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    track_colors.len().hash(&mut h);
    for &(idx, _) in track_colors {
        idx.hash(&mut h);
    }
    h.finish()
}

/// 从 cells + 当前颜色配置重建 output（颜色快速路径）
///
/// O(C) 遍历，C = cell 数量。远快于 O(N×T) 全量重查。
///
/// 优化：预打包颜色 LUT，pack_color 每音轨只做一次，避免逐 cell 重复打包。
pub(super) fn rebuild_output_from_cells(
    cells: &HashMap<u64, MergedCell>,
    track_colors: &[(usize, [f32; 4])],
) -> std::sync::Arc<Vec<NoteInstance>> {
    // 构建预打包颜色查找表：track_idx → u32（pack_color 每轨只做一次）
    let max_idx = track_colors.iter().map(|&(idx, _)| idx).max().unwrap_or(0);
    let mut color_lut: Vec<u32> = vec![pack_color([0.5, 0.5, 0.5, 0.4]); max_idx + 1];
    for &(idx, color) in track_colors {
        color_lut[idx] = pack_color(color);
    }

    std::sync::Arc::new(
        cells
            .values()
            .map(|c| {
                let color_packed = color_lut[c.track_idx as usize];
                NoteInstance {
                    position: [c.tick_start, c.key],
                    size_x: c.max_right - c.tick_start,
                    color_packed,
                }
            })
            .collect(),
    )
}
