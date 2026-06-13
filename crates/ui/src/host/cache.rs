use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::editor::onion_bg_pool::OnionBgTilePool;
use iced_wgpu::wgpu;
use lumino_gfx::{OnionBgTileRef, OnionNote, SwappableBuffer};

/// 渲染缓存 - 避免每帧重复上传相同数据
///
/// 使用双缓冲机制实现 UI 线程和渲染线程的零拷贝数据共享：
/// - Back Buffer: UI 线程写入音符实例数据
/// - Front Buffer: 渲染线程读取并上传到 GPU
/// - 交换操作: 原子指针交换，无数据拷贝
pub struct RenderCache {
    /// 缓存的网格线实例
    pub grid_instances: Vec<lumino_gfx::GridLineInstance>,
    /// 双缓冲主音符实例数据（UI线程写入，渲染线程读取）
    pub note_instances_buffer: Arc<SwappableBuffer<lumino_gfx::NoteInstance>>,
    /// 双缓冲洋葱皮背景瓦片引用（Worker线程写入，渲染线程读取）
    pub onion_bg_tiles_buffer: Arc<SwappableBuffer<OnionBgTileRef>>,
    /// 双缓冲洋葱皮音符池（SoA 布局，用于 GPU 计算剔除渲染）
    pub onion_note_buffer: Arc<SwappableBuffer<OnionNote>>,
    /// 主音符版本号（用于检测数据变化）
    pub note_instances_version: u64,
    /// 网格线视口哈希（用于检测变化）
    pub grid_viewport_hash: u64,
    /// 音符视口哈希（用于检测变化）
    pub note_viewport_hash: u64,
    /// 洋葱皮视口哈希（用于检测显著变化，带量化节流）
    pub onion_viewport_hash: u64,
    /// 缓存的深度纹理 (宽, 高, view)
    pub depth_texture: Option<(u32, u32, wgpu::TextureView)>,
    /// 洋葱皮背景瓦片池（主线程创建，NoteWorker 与 WGPU 线程共享）
    pub tile_pool: Option<Arc<Mutex<OnionBgTilePool>>>,
    /// 走带视图实例缓存（避免每帧重建）
    pub arrangement_instances: Vec<lumino_gfx::ArrangementNoteInstance>,
    // ── 洋葱皮快照 Arc 缓存（避免每帧克隆 track_notes HashMap） ──
    /// 缓存的 track_notes Arc，避免每帧全量克隆 HashMap。
    /// 当 `track_notes_gen` 不变时直接 clone Arc（O(1) 引用计数递增）。
    pub cached_track_notes_arc: Option<Arc<HashMap<usize, im::Vector<crate::editor::note::Note>>>>,
    /// 上一次缓存时的 track_notes_gen 值，用于检测变化。
    pub cached_track_notes_gen: u64,
}

impl RenderCache {
    pub fn new() -> Self {
        Self {
            grid_instances: Vec::new(),
            note_instances_buffer: Arc::new(SwappableBuffer::new(1024 * 1024)),
            onion_bg_tiles_buffer: Arc::new(SwappableBuffer::new(1024)),
            onion_note_buffer: Arc::new(SwappableBuffer::new(256 * 1024)),
            note_instances_version: 0,
            grid_viewport_hash: 0,
            note_viewport_hash: 0,
            onion_viewport_hash: 0,
            depth_texture: None,
            tile_pool: None,
            arrangement_instances: Vec::new(),
            cached_track_notes_arc: None,
            cached_track_notes_gen: u64::MAX, // 初始值确保首次触发重建
        }
    }

    /// 获取音符实例数量（从双缓冲的前缓冲区）
    pub fn note_instances_len(&self) -> usize {
        unsafe { self.note_instances_buffer.read_buffer().len() }
    }

    /// 检查音符实例是否为空
    pub fn note_instances_is_empty(&self) -> bool {
        unsafe { self.note_instances_buffer.read_buffer().is_empty() }
    }

    /// 获取或创建 track_notes 的 Arc 缓存。
    ///
    /// 当 `current_gen` 与缓存版本匹配时，直接 clone 缓存的 Arc（O(1)），
    /// 避免每帧全量克隆 HashMap。仅当版本变化时才从 source 新建 Arc。
    ///
    /// # 参数
    /// * `source` - 用于构建 Arc 的数据源（仅在 gen 变化时访问）
    /// * `current_gen` - 当前数据源的版本号
    ///
    /// # 返回
    /// 一个 Arc 包裹的 track_notes HashMap
    pub fn get_or_create_track_notes_arc(
        &mut self,
        source: &HashMap<usize, im::Vector<crate::editor::note::Note>>,
        current_gen: u64,
    ) -> Arc<HashMap<usize, im::Vector<crate::editor::note::Note>>> {
        // 缓存命中：gen 未变，直接 clone Arc
        if let Some(ref cached) = self.cached_track_notes_arc
            && self.cached_track_notes_gen == current_gen
        {
            return Arc::clone(cached);
        }

        // 缓存未命中：从 source 构建新的 Arc
        let new_arc = Arc::new(source.clone());
        self.cached_track_notes_arc = Some(Arc::clone(&new_arc));
        self.cached_track_notes_gen = current_gen;
        new_arc
    }

    /// 计算洋葱皮视口哈希（带量化节流）
    ///
    /// 使用量化的 scroll/zoom 值，使得微小移动不触发 OS 重算。
    /// 量化粒度：scroll 方向 32 像素，zoom 方向 0.1 倍。
    pub fn compute_onion_viewport_hash(
        scroll_x: f32,
        scroll_y: f32,
        zoom_x: f32,
        zoom_y: f32,
        canvas_x: f32,
        canvas_y: f32,
        visible_key_count: u16,
    ) -> u64 {
        fn hash_compose(state: u64, val: u64) -> u64 {
            state.wrapping_mul(0x9e3779b97f4a7c15).wrapping_add(val)
        }

        // 量化：scroll 按 32px 取整，zoom 保留 1 位小数
        let q_scroll_x = (scroll_x / 32.0).round() as i64;
        let q_scroll_y = (scroll_y / 32.0).round() as i64;
        let q_zoom_x = (zoom_x * 10.0).round() as i64;
        let q_zoom_y = (zoom_y * 10.0).round() as i64;

        let mut hash: u64 = 9_871_654_321_098_765;
        hash = hash_compose(hash, q_scroll_x as u64);
        hash = hash_compose(hash, q_scroll_y as u64);
        hash = hash_compose(hash, q_zoom_x as u64);
        hash = hash_compose(hash, q_zoom_y as u64);
        hash = hash_compose(hash, canvas_x.to_bits() as u64);
        hash = hash_compose(hash, canvas_y.to_bits() as u64);
        hash = hash_compose(hash, visible_key_count as u64);
        hash
    }

    /// 计算视口哈希（滚动+缩放+画布大小+可见键数）
    pub fn compute_viewport_hash(
        scroll_x: f32,
        scroll_y: f32,
        zoom_x: f32,
        zoom_y: f32,
        canvas_x: f32,
        canvas_y: f32,
        visible_key_count: u16,
    ) -> u64 {
        fn hash_compose(state: u64, val: u64) -> u64 {
            state.wrapping_mul(0x9e3779b97f4a7c15).wrapping_add(val)
        }

        let mut hash: u64 = 3_154_789_634_698_251_631;
        hash = hash_compose(hash, scroll_x.to_bits() as u64);
        hash = hash_compose(hash, scroll_y.to_bits() as u64);
        hash = hash_compose(hash, zoom_x.to_bits() as u64);
        hash = hash_compose(hash, zoom_y.to_bits() as u64);
        hash = hash_compose(hash, canvas_x.to_bits() as u64);
        hash = hash_compose(hash, canvas_y.to_bits() as u64);
        hash = hash_compose(hash, visible_key_count as u64);
        hash
    }
}

impl Default for RenderCache {
    fn default() -> Self {
        Self::new()
    }
}
