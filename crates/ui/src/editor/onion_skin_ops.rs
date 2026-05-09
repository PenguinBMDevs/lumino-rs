use std::sync::LazyLock;

use crate::editor::note::Note;
use crate::editor::{CacheInvalidation, Editor};
use lumino_gfx::NoteInstance;
use rayon::prelude::*;

/// 专用 rayon 线程池，避免与 UI/iced 的全局线程池竞争。
/// 只用于洋葱皮音轨的并行查询+合并。
static ONION_SKIN_POOL: LazyLock<rayon::ThreadPool> = LazyLock::new(|| {
    rayon::ThreadPoolBuilder::new()
        .thread_name(|i| format!("onion-skin-{}", i))
        .build()
        .expect("Failed to create onion skin thread pool")
});

/// 洋葱皮结果缓存：记录上一帧的合并结果和视口。
/// 滚动的帧只做增量更新（补新进视口的 cell），不做 799 轨全量重查。
struct OnionSkinCache {
    /// 量化值
    tick_quant: u32,
    /// 视口范围
    search_start: f32,
    search_end: f32,
    search_key_min: u16,
    search_key_max: u16,
    /// 音轨配置哈希（可见音轨 + 颜色）
    config_hash: u64,
    /// 合并后的格子
    cells: std::collections::HashMap<u64, MergedCell>,
    /// 输出缓存
    output: Vec<NoteInstance>,
}

static ONION_SKIN_CACHE: LazyLock<std::sync::Mutex<Option<OnionSkinCache>>> =
    LazyLock::new(|| std::sync::Mutex::new(None));

/// 合并中的格子状态：在同一格子内的音符合并成一个大矩形
/// 使用 tick_start == EMPTY 标记空格子，规避 Option 对齐开销
#[derive(Copy, Clone)]
struct MergedCell {
    tick_start: f32,
    max_right: f32,
    key: f32,
    color: [f32; 4],
}

impl MergedCell {
    #[allow(dead_code)]
    const EMPTY: f32 = f32::MAX;

    #[allow(dead_code)]
    const fn empty() -> Self {
        Self {
            tick_start: Self::EMPTY,
            max_right: 0.0,
            key: 0.0,
            color: [0.0; 4],
        }
    }

    #[allow(dead_code)]
    fn is_empty(&self) -> bool {
        self.tick_start == Self::EMPTY
    }
}

/// 将单个音轨的原始音符合并到 HashMap
fn merge_one_track(
    raw: &[(f32, u8, f32, u8, u8)],
    search_start: f32,
    search_end: f32,
    search_key_min: u16,
    search_key_max: u16,
    tick_quant: u32,
    color_arr: [f32; 4],
) -> Option<std::collections::HashMap<u64, MergedCell>> {
    let mut cells = std::collections::HashMap::new();

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
                    m.color = color_arr;
                })
                .or_insert(MergedCell {
                    tick_start: tick,
                    max_right: right,
                    key: key as f32,
                    color: color_arr,
                });
        }
    }

    if cells.is_empty() { None } else { Some(cells) }
}

/// 将 cell 合并到已有的 HashMap（后入的为上层，覆盖颜色）
fn merge_cell(acc: &mut std::collections::HashMap<u64, MergedCell>, key: u64, cell: MergedCell) {
    match acc.entry(key) {
        std::collections::hash_map::Entry::Occupied(mut e) => {
            let m = e.get_mut();
            if cell.tick_start < m.tick_start {
                m.tick_start = cell.tick_start;
            }
            if cell.max_right > m.max_right {
                m.max_right = cell.max_right;
            }
            m.color = cell.color;
        }
        std::collections::hash_map::Entry::Vacant(e) => {
            e.insert(cell);
        }
    }
}

impl OnionSkinCache {
    fn can_incremental(
        &self,
        tick_quant: u32,
        search_start: f32,
        search_end: f32,
        search_key_min: u16,
        search_key_max: u16,
        config_hash: u64,
    ) -> bool {
        // 配置变了 → 不可增量
        if self.config_hash != config_hash {
            tracing::info!("CACHE MISS: config_hash changed");
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
        // key 范围变了 → 不可增量
        if self.search_key_min != search_key_min {
            tracing::info!(
                "CACHE MISS: key_min {} != {}",
                self.search_key_min,
                search_key_min
            );
            return false;
        }
        if self.search_key_max != search_key_max {
            tracing::info!(
                "CACHE MISS: key_max {} != {}",
                self.search_key_max,
                search_key_max
            );
            return false;
        }
        // 视口完全没变 → 直接命中缓存
        if self.search_start == search_start && self.search_end == search_end {
            return true;
        }
        // 视口偏移太大（> 30% 缓存宽度） → 全量重查更快
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

impl Editor {
    /// 获取洋葱皮配置的可变引用
    pub fn onion_skin_config_mut(&mut self) -> &mut super::OnionSkinConfig {
        &mut self.onion_skin_config
    }

    /// 获取洋葱皮配置的引用
    pub fn onion_skin_config(&self) -> &super::OnionSkinConfig {
        &self.onion_skin_config
    }

    /// 启用洋葱皮
    pub fn enable_onion_skin(&mut self) {
        self.onion_skin_config.enable();
        self.invalidate_caches(CacheInvalidation::GRID);
        tracing::debug!("Editor: 洋葱皮已启用");
    }

    /// 禁用洋葱皮
    pub fn disable_onion_skin(&mut self) {
        self.onion_skin_config.disable();
        self.invalidate_caches(CacheInvalidation::GRID);
        tracing::debug!("Editor: 洋葱皮已禁用");
    }

    /// 切换洋葱皮开关
    pub fn toggle_onion_skin(&mut self) {
        self.onion_skin_config.toggle();
        self.invalidate_caches(CacheInvalidation::GRID);
        tracing::info!(
            "Editor: 洋葱皮已切换, is_enabled={}",
            self.onion_skin_config.is_enabled()
        );
    }

    /// 检查洋葱皮是否启用
    pub fn is_onion_skin_enabled(&self) -> bool {
        self.onion_skin_config.is_enabled()
    }

    /// 设置音轨的洋葱皮颜色
    pub fn set_onion_skin_color(&mut self, track_idx: usize, color: iced_core::Color) {
        self.onion_skin_config.set_track_color(track_idx, color);
        self.invalidate_caches(CacheInvalidation::GRID);
    }

    /// 获取音轨的洋葱皮颜色
    pub fn get_onion_skin_color(&self, track_idx: usize) -> iced_core::Color {
        self.onion_skin_config.get_track_color(track_idx)
    }

    /// 设置洋葱皮透明度
    pub fn set_onion_skin_opacity(&mut self, opacity: f32) {
        self.onion_skin_config.set_opacity(opacity);
        self.invalidate_caches(CacheInvalidation::GRID);
    }

    /// 获取洋葱皮透明度
    pub fn onion_skin_opacity(&self) -> f32 {
        self.onion_skin_config.opacity()
    }

    /// 设置是否显示所有音轨的洋葱皮
    pub fn set_onion_skin_show_all(&mut self, show_all: bool) {
        self.onion_skin_config.set_show_all_tracks(show_all);
        self.invalidate_caches(CacheInvalidation::GRID);
    }

    /// 添加可见音轨到洋葱皮
    pub fn add_onion_skin_track(&mut self, track_idx: usize) {
        self.onion_skin_config.add_visible_track(track_idx);
        self.invalidate_caches(CacheInvalidation::GRID);
    }

    /// 从洋葱皮移除音轨
    pub fn remove_onion_skin_track(&mut self, track_idx: usize) {
        self.onion_skin_config.remove_visible_track(track_idx);
        self.invalidate_caches(CacheInvalidation::GRID);
    }

    /// 获取所有洋葱皮音符原始数据（用于缓存）
    /// 返回 (tick, key, length, color) 元组，不含屏幕坐标
    ///
    /// 纯流式处理，无数量限制，确保黑乐谱完整显示。
    pub fn get_onion_skin_notes(
        &self,
        track_onion_states: &std::collections::HashMap<usize, bool>,
        visible_tick_start: f32,
        visible_tick_end: f32,
        visible_key_min: u16,
        visible_key_max: u16,
    ) -> Vec<(f32, u16, f32, iced_core::Color)> {
        if !self.is_onion_skin_enabled() {
            return Vec::new();
        }

        let Some(doc) = self.editor_state.data.document.as_ref() else {
            return Vec::new();
        };

        let track_indices = self.collect_visible_track_indices(track_onion_states);
        if track_indices.is_empty() {
            return Vec::new();
        }

        // 搜索范围 = 视口范围
        let search_start = visible_tick_start;
        let search_end = visible_tick_end;

        // 预收集音轨颜色和启用状态，避免在闭包中访问 self
        let track_configs: Vec<(usize, bool, iced_core::Color)> = track_indices
            .iter()
            .filter_map(|&track_idx| {
                let is_enabled = *track_onion_states.get(&track_idx)?;
                if !self
                    .onion_skin_config
                    .should_show_track(track_idx, is_enabled)
                {
                    return None;
                }
                let color = self.onion_skin_config.get_track_color(track_idx);
                Some((track_idx, is_enabled, color))
            })
            .collect();

        // 并行处理音轨查询 - 纯流式处理，无数量限制
        let all_notes: Vec<(f32, u16, f32, iced_core::Color)> = track_configs
            .par_iter()
            .filter_map(|&(track_idx, _is_enabled, color)| {
                // 快速检查：音轨在视口范围内是否有事件
                if !doc.has_track_events_in_range(
                    track_idx as u16,
                    search_start as u32,
                    search_end as u32,
                ) {
                    return None;
                }

                // 直接使用 Document 查询（二分查找，无索引构建开销）
                let raw = doc.get_track_notes_in_range(track_idx as u16, search_start, search_end);
                if raw.is_empty() {
                    return None;
                }

                // 纯流式处理：直接构建结果，无限制
                let mut track_notes = Vec::with_capacity(raw.len());

                for &(tick, key, length, _vel, _ch) in raw.iter() {
                    let key_u16 = key as u16;
                    if key_u16 >= visible_key_min
                        && key_u16 <= visible_key_max
                        && tick + length >= visible_tick_start
                        && tick <= visible_tick_end
                    {
                        track_notes.push((tick, key_u16, length, color));
                    }
                }

                if track_notes.is_empty() {
                    None
                } else {
                    Some(track_notes)
                }
            })
            .reduce(
                Vec::new,
                |mut a, mut b| {
                    // 如果 a 为空，直接返回 b
                    if a.is_empty() {
                        return b;
                    }
                    a.append(&mut b);
                    a
                },
            );

        all_notes
    }

    /// 收集可见音轨索引
    ///
    /// 返回降序排列的音轨索引，确保最后一个音轨渲染在最底层（第一层洋葱皮），
    /// 第一个音轨渲染在最顶层（最后一层洋葱皮），避免闪烁问题。
    fn collect_visible_track_indices(
        &self,
        track_onion_states: &std::collections::HashMap<usize, bool>,
    ) -> Vec<usize> {
        let mut indices: Vec<usize> = track_onion_states
            .iter()
            .filter(|(_, is_enabled)| **is_enabled)
            .map(|(&idx, _)| idx)
            .filter(|&idx| idx != self.editor_state.data.current_track)
            .collect();
        indices.sort_by(|a, b| b.cmp(a)); // 降序排列：大索引先渲染（在底层），小索引后渲染（在顶层）
        indices
    }

    /// 获取洋葱皮音符实例（用于其他音轨的音符显示）
    /// 音符直接送入 wgpu 渲染管线，GPU compute shader 负责视锥裁剪
    pub fn get_onion_skin_instances(
        &mut self,
        track_idx: usize,
        track_onion_enabled: bool,
    ) -> Vec<NoteInstance> {
        if !self
            .onion_skin_config
            .should_show_track(track_idx, track_onion_enabled)
        {
            return Vec::new();
        }

        if track_idx == self.editor_state.data.current_track {
            return Vec::new();
        }

        // 先将所有音符做成 NoteInstance（GPU shader 负责裁剪）
        // 使用 closure 构建实例列表，同时处理 cache hit/miss
        let make_instances =
            |notes: &im::Vector<Note>, color: iced_core::Color| -> Vec<NoteInstance> {
                let mut instances = Vec::with_capacity(notes.len());
                for note in notes.iter() {
                    instances.push(note.to_instance(color));
                }
                instances
            };

        let color = self.onion_skin_config.get_track_color(track_idx);

        // 先查 track_notes 缓存
        if let Some(cached) = self.editor_state.data.track_notes.get(&track_idx) {
            if cached.is_empty() {
                return Vec::new();
            }
            return make_instances(cached, color);
        }

        // 缓存未命中 → 从 document 加载并缓存
        let Some(doc) = self.editor_state.data.document.as_ref() else {
            return Vec::new();
        };
        if track_idx as u16 >= doc.track_count() as u16 {
            return Vec::new();
        }
        if doc.track_note_count(track_idx as u16) == 0 {
            return Vec::new();
        }
        let raw = doc.get_track_notes(track_idx as u16);
        if raw.is_empty() {
            return Vec::new();
        }

        let mut notes: im::Vector<Note> = im::Vector::new();
        for (tick, key, length, velocity, channel) in &raw {
            notes.push_back(
                Note::new(*tick, *key as u16, *length)
                    .with_velocity(*velocity)
                    .with_channel(*channel),
            );
        }
        self.editor_state
            .data
            .track_notes
            .insert(track_idx, notes.clone());

        make_instances(&notes, color)
    }

    /// 计算音轨配置哈希（用于缓存失效检测）
    fn track_config_hash(track_colors: &[(usize, [f32; 4])]) -> u64 {
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

    /// 获取所有洋葱皮音符实例（视口范围内）—— 增量缓存版
    ///
    /// 核心优化：缓存上一帧的合并结果。滚动的帧只做增量更新：
    /// 1. 移除离开视口的 cell
    /// 2. 只查询新进入视口的 tick 范围（通常 ~10 ticks）
    /// 3. 合并新 cell 到缓存
    ///
    /// 避免 799 个音轨全量重查，16M 事件迭代。
    pub fn get_all_onion_skin_instances_in_range(
        &mut self,
        track_onion_states: &std::collections::HashMap<usize, bool>,
        visible_tick_start: f32,
        visible_tick_end: f32,
        visible_key_min: u16,
        visible_key_max: u16,
    ) -> Vec<NoteInstance> {
        if !self.is_onion_skin_enabled() {
            return Vec::new();
        }

        let Some(doc) = self.editor_state.data.document.as_ref() else {
            return Vec::new();
        };

        let search_start = visible_tick_start;
        let search_end = visible_tick_end;
        let search_key_min = visible_key_min;
        let search_key_max = visible_key_max;

        let track_indices = self.collect_visible_track_indices(track_onion_states);
        if track_indices.is_empty() {
            return Vec::new();
        }

        let tick_span = (search_end - search_start) as u32;
        let tick_quant = (tick_span / 100).max(1);

        // 预收集音轨颜色
        let track_colors: Vec<(usize, [f32; 4])> = track_indices
            .iter()
            .map(|&track_idx| {
                let color = self.onion_skin_config.get_track_color(track_idx);
                let color_arr = super::note::color_to_array(color);
                (track_idx, color_arr)
            })
            .collect();

        let config_hash = Self::track_config_hash(&track_colors);

        // === 尝试增量缓存 ===
        let mut cache_guard = ONION_SKIN_CACHE.lock().unwrap();

        if let Some(ref mut cache) = *cache_guard
            && cache.can_incremental(
                tick_quant,
                search_start,
                search_end,
                search_key_min,
                search_key_max,
                config_hash,
            ) {
                // 1. 移除离开视口的 cell
                cache.cells.retain(|_, cell| {
                    cell.max_right >= search_start && cell.tick_start <= search_end
                });

                // 2. 确定新进入视口的 tick 范围
                let (new_start, new_end) = if search_start > cache.search_start {
                    // 向右滚：新区在右
                    (cache.search_end, search_end)
                } else {
                    // 向左滚：新区在左
                    (search_start, cache.search_start)
                };

                // 3. 只查询新区间
                if new_end > new_start {
                    let new_cells = ONION_SKIN_POOL.install(|| {
                        track_colors
                            .par_iter()
                            .filter_map(|&(track_idx, color_arr)| {
                                let raw = doc.get_track_notes_in_range(
                                    track_idx as u16,
                                    new_start,
                                    new_end,
                                );
                                if raw.is_empty() {
                                    return None;
                                }
                                merge_one_track(
                                    &raw,
                                    new_start,
                                    new_end,
                                    search_key_min,
                                    search_key_max,
                                    tick_quant,
                                    color_arr,
                                )
                            })
                            .reduce(
                                std::collections::HashMap::new,
                                |mut acc, cells| {
                                    for (k, v) in cells {
                                        merge_cell(&mut acc, k, v);
                                    }
                                    acc
                                },
                            )
                    });

                    // 合并新区 cell 到缓存
                    for (k, v) in new_cells {
                        merge_cell(&mut cache.cells, k, v);
                    }
                }

                cache.search_start = search_start;
                cache.search_end = search_end;

                // 构建输出 Vec
                cache.output.clear();
                cache.output.extend(cache.cells.values().map(|c| {
                    NoteInstance::new(c.tick_start, c.key, c.max_right - c.tick_start, c.color)
                }));

                tracing::info!(
                    "Onion skin (incremental): {} cells, new_range=[{},{}]",
                    cache.output.len(),
                    new_start,
                    new_end,
                );

                return cache.output.clone();
            }

        // === 全量重建 ===
        let merged_map: std::collections::HashMap<u64, MergedCell> =
            ONION_SKIN_POOL.install(|| {
                track_colors
                    .par_iter()
                    .filter_map(|&(track_idx, color_arr)| {
                        let raw = doc.get_track_notes_in_range(
                            track_idx as u16,
                            search_start,
                            search_end,
                        );
                        if raw.is_empty() {
                            return None;
                        }
                        merge_one_track(
                            &raw,
                            search_start,
                            search_end,
                            search_key_min,
                            search_key_max,
                            tick_quant,
                            color_arr,
                        )
                    })
                    .reduce(
                        std::collections::HashMap::new,
                        |mut acc, cells| {
                            for (k, v) in cells {
                                merge_cell(&mut acc, k, v);
                            }
                            acc
                        },
                    )
            });

        let result: Vec<NoteInstance> = merged_map
            .values()
            .map(|c| NoteInstance::new(c.tick_start, c.key, c.max_right - c.tick_start, c.color))
            .collect();

        tracing::info!(
            "Onion skin (full): {} tracks → {} instances (quant={})",
            track_indices.len(),
            result.len(),
            tick_quant,
        );

        // 存入缓存
        *cache_guard = Some(OnionSkinCache {
            tick_quant,
            search_start,
            search_end,
            search_key_min,
            search_key_max,
            config_hash,
            cells: merged_map,
            output: result.clone(),
        });

        result
    }

    /// 从 document 加载音轨音符到 track_notes 缓存
    #[allow(dead_code)]
    fn load_track_notes_from_document(&mut self, track_idx: usize) {
        let Some(doc) = self.editor_state.data.document.as_ref() else {
            return;
        };
        if track_idx as u16 >= doc.track_count() as u16 {
            return;
        }
        if doc.track_note_count(track_idx as u16) == 0 {
            self.editor_state
                .data
                .track_notes
                .insert(track_idx, im::Vector::new());
            return;
        }
        let raw = doc.get_track_notes(track_idx as u16);
        if raw.is_empty() {
            self.editor_state
                .data
                .track_notes
                .insert(track_idx, im::Vector::new());
            return;
        }

        let mut notes: im::Vector<Note> = im::Vector::new();
        for (tick, key, length, velocity, channel) in &raw {
            notes.push_back(
                Note::new(*tick, *key as u16, *length)
                    .with_velocity(*velocity)
                    .with_channel(*channel),
            );
        }
        self.editor_state.data.track_notes.insert(track_idx, notes);
    }

    /// 获取所有洋葱皮音符实例（所有其他音轨）
    ///
    /// 音符全部送入 wgpu 管线，GPU compute shader 负责视锥裁剪。
    /// 音轨按降序处理，确保最后一个音轨渲染在最底层（第一层洋葱皮），
    /// 第一个音轨渲染在最顶层（最后一层洋葱皮），避免闪烁问题。
    pub fn get_all_onion_skin_instances(
        &mut self,
        track_onion_states: &std::collections::HashMap<usize, bool>,
    ) -> Vec<NoteInstance> {
        if !self.is_onion_skin_enabled() {
            return Vec::new();
        }

        let mut track_indices: Vec<usize> = track_onion_states
            .iter()
            .filter(|(_, is_enabled)| **is_enabled)
            .map(|(&idx, _)| idx)
            .filter(|&idx| idx != self.editor_state.data.current_track)
            .collect();

        track_indices.sort_by(|a, b| b.cmp(a)); // 降序排列：大索引先渲染（在底层），小索引后渲染（在顶层）

        let mut all_instances = Vec::new();
        for track_idx in track_indices {
            if let Some(&is_enabled) = track_onion_states.get(&track_idx) {
                all_instances.extend(self.get_onion_skin_instances(track_idx, is_enabled));
            }
        }

        all_instances
    }
}
