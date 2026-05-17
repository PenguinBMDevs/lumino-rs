use std::sync::{Arc, Mutex};

use iced_wgpu::wgpu;
use lumino_gfx::{OnionBgTileRef, SwappableBuffer};
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
    ///
    /// 使用 Arc 以便在分离渲染线程中共享给渲染线程
    pub note_instances_buffer: Arc<SwappableBuffer<lumino_gfx::NoteInstance>>,
    /// 双缓冲洋葱皮实例数据（Worker线程写入，渲染线程读取）
    ///
    /// 与主音符分离，避免一帧内多次 swap 导致闪烁
    pub onion_skin_instances_buffer: Arc<SwappableBuffer<lumino_gfx::NoteInstance>>,
    /// 双缓冲洋葱皮背景瓦片引用（Worker线程写入，渲染线程读取）
    pub onion_bg_tiles_buffer: Arc<SwappableBuffer<OnionBgTileRef>>,
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

/// 注意：这些方法会触发双缓冲交换，应该只在渲染线程调用
impl RenderCache {
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
}

impl Default for RenderCache {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderCache {
    /// 初始容量：10000 个音符实例（约 640KB）
    const INITIAL_NOTE_CAPACITY: usize = 10000;

    pub fn new() -> Self {
        Self {
            grid_instances: Vec::new(),
            note_instances_buffer: Arc::new(SwappableBuffer::new(Self::INITIAL_NOTE_CAPACITY)),
            onion_skin_instances_buffer: Arc::new(SwappableBuffer::new(
                Self::INITIAL_NOTE_CAPACITY,
            )),
            onion_bg_tiles_buffer: Arc::new(SwappableBuffer::new(1024)),
            note_instances_version: 0,
            onion_skin_instances_version: 0,
            tile_pool: None,
            grid_viewport_hash: 0,
            note_viewport_hash: 0,
            depth_texture: None,
        }
    }

    /// 计算视口状态的哈希值
    pub fn compute_viewport_hash(
        scroll_x: f32,
        scroll_y: f32,
        zoom_x: f32,
        zoom_y: f32,
        canvas_width: f32,
        canvas_height: f32,
        visible_key_count: u16,
    ) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        scroll_x.to_bits().hash(&mut hasher);
        scroll_y.to_bits().hash(&mut hasher);
        zoom_x.to_bits().hash(&mut hasher);
        zoom_y.to_bits().hash(&mut hasher);
        canvas_width.to_bits().hash(&mut hasher);
        canvas_height.to_bits().hash(&mut hasher);
        visible_key_count.hash(&mut hasher);
        hasher.finish()
    }
}
