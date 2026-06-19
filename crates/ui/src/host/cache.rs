use std::sync::Arc;

use iced_wgpu::wgpu;
use lumino_gfx::{OnionBgTileRef, OnionNoteList, SwappableBuffer};

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
    pub tile_pool: Option<Arc<std::sync::Mutex<crate::editor::onion_bg_pool::OnionBgTilePool>>>,
    /// 走带视图实例缓存（避免每帧重建）
    pub arrangement_instances: Vec<lumino_gfx::ArrangementNoteInstance>,
    /// 洋葱皮音符列表（从 Wasabi 瀑布流简化而来，扁平存储）
    pub onion_note_list: Option<Arc<OnionNoteList>>,
    /// 上一次构建 note list 时的 document Arc 指针
    pub onion_list_doc_ptr: Option<*const ()>,
    /// 上一次构建 note list 时的 track_notes_gen
    pub onion_list_track_gen: u64,
    /// 缓存的洋葱皮 per-track 打包颜色（避免每帧重建）
    pub onion_track_colors: Option<Vec<u32>>,
    /// 缓存颜色对应的 OnionSkinColors 版本号
    pub onion_colors_version: u64,
}

impl RenderCache {
    pub fn new() -> Self {
        Self {
            grid_instances: Vec::new(),
            note_instances_buffer: Arc::new(SwappableBuffer::new(1024 * 1024)),
            onion_bg_tiles_buffer: Arc::new(SwappableBuffer::new(1024)),
            note_instances_version: 0,
            grid_viewport_hash: 0,
            note_viewport_hash: 0,
            onion_viewport_hash: 0,
            depth_texture: None,
            tile_pool: None,
            arrangement_instances: Vec::new(),
            onion_note_list: None,
            onion_list_doc_ptr: None,
            onion_list_track_gen: u64::MAX,
            onion_track_colors: None,
            onion_colors_version: u64::MAX,
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
