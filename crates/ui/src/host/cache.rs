use std::sync::{Arc, Mutex};

use iced_wgpu::wgpu;
use lumino_gfx::{OnionBgTileRef, OnionNote, SwappableBuffer};
use crate::editor::onion_bg_pool::OnionBgTilePool;

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
    /// 双缓冲洋葱皮实例数据（Worker线程写入，渲染线程读取）
    pub onion_skin_instances_buffer: Arc<SwappableBuffer<lumino_gfx::NoteInstance>>,
    /// 双缓冲洋葱皮背景瓦片引用（Worker线程写入，渲染线程读取）
    pub onion_bg_tiles_buffer: Arc<SwappableBuffer<OnionBgTileRef>>,
    /// 双缓冲洋葱皮音符池（SoA 布局，用于 GPU 计算剔除渲染）
    pub onion_note_buffer: Arc<SwappableBuffer<OnionNote>>,
    /// 主音符版本号（用于检测数据变化）
    pub note_instances_version: u64,
    /// 洋葱皮版本号（用于检测数据变化）
    pub onion_skin_instances_version: u64,
    /// 网格线视口哈希（用于检测变化）
    pub grid_viewport_hash: u64,
    /// 音符视口哈希（用于检测变化）
    pub note_viewport_hash: u64,
    /// 缓存的深度纹理 (宽, 高, view)
    pub depth_texture: Option<(u32, u32, wgpu::TextureView)>,
    /// 洋葱皮背景瓦片池（主线程创建，NoteWorker 与 WGPU 线程共享）
    pub tile_pool: Option<Arc<Mutex<OnionBgTilePool>>>,
}

impl RenderCache {
    pub fn new() -> Self {
        Self {
            grid_instances: Vec::new(),
            note_instances_buffer: Arc::new(SwappableBuffer::new(1024 * 1024)),
            onion_skin_instances_buffer: Arc::new(SwappableBuffer::new(256 * 1024)),
            onion_bg_tiles_buffer: Arc::new(SwappableBuffer::new(1024)),
            onion_note_buffer: Arc::new(SwappableBuffer::new(256 * 1024)),
            note_instances_version: 0,
            onion_skin_instances_version: 0,
            grid_viewport_hash: 0,
            note_viewport_hash: 0,
            depth_texture: None,
            tile_pool: None,
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

    /// 获取洋葱皮实例数量
    pub fn onion_skin_instances_len(&self) -> usize {
        unsafe { self.onion_skin_instances_buffer.read_buffer().len() }
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