use std::sync::Arc;

use wgpu;

use crate::swappable_buffer::SwappableBuffer;
use crate::{ArrangementNoteInstance, NoteInstance};

/// 音符渲染视口缓存（带 overscan）
///
/// 记录上一次渲染时使用的扩展视口范围，用于判断当前可见视口
/// 是否仍在缓存范围内，避免频繁重建音符实例。
#[derive(Debug, Clone, Copy)]
pub struct NoteRenderViewport {
    /// 扩展后的 tick 起始
    pub tick_start: f32,
    /// 扩展后的 tick 结束
    pub tick_end: f32,
    /// 扩展后的 key 最小值
    pub key_min: u16,
    /// 扩展后的 key 最大值
    pub key_max: u16,
}

impl NoteRenderViewport {
    /// 检查给定视口是否完全包含在此缓存视口内
    #[inline]
    pub fn contains(&self, tick_start: f32, tick_end: f32, key_min: u16, key_max: u16) -> bool {
        self.tick_start <= tick_start
            && self.tick_end >= tick_end
            && self.key_min <= key_min
            && self.key_max >= key_max
    }
}

/// 渲染缓存 - 避免每帧重复上传相同数据
///
/// 使用双缓冲机制实现 UI 线程和渲染线程的零拷贝数据共享：
/// - Back Buffer: UI 线程写入音符实例数据
/// - Front Buffer: 渲染线程读取并上传到 GPU
/// - 交换操作: 原子指针交换，无数据拷贝
pub struct RenderCache {
    /// 双缓冲主音符实例数据（UI线程写入，渲染线程读取）
    pub note_instances_buffer: Arc<SwappableBuffer<NoteInstance>>,
    /// 主音符版本号（用于检测数据变化）
    pub note_instances_version: u64,
    /// 网格线视口哈希（用于检测变化）
    pub grid_viewport_hash: u64,
    /// 音符视口哈希（用于检测变化）
    pub note_viewport_hash: u64,
    /// 上一次渲染使用的扩展视口范围
    pub note_render_viewport: Option<NoteRenderViewport>,
    /// 可见音符数据临时缓冲（避免每帧重新分配）
    pub visible_notes_buffer: Vec<(f32, u16, f32)>,
    /// 缓存的深度纹理 (宽, 高, view)
    pub depth_texture: Option<(u32, u32, wgpu::TextureView)>,
    /// 走带视图实例缓存（避免每帧重建）
    pub arrangement_instances: Vec<ArrangementNoteInstance>,
}

impl RenderCache {
    pub fn new() -> Self {
        Self {
            note_instances_buffer: Arc::new(SwappableBuffer::new(1024 * 1024)),
            note_instances_version: 0,
            grid_viewport_hash: 0,
            note_viewport_hash: 0,
            note_render_viewport: None,
            visible_notes_buffer: Vec::new(),
            depth_texture: None,
            arrangement_instances: Vec::new(),
        }
    }

    /// 获取音符实例数量（从 reading_len 原子量读取，不碰状态机）
    pub fn note_instances_len(&self) -> usize {
        self.note_instances_buffer
            .reading_len
            .load(std::sync::atomic::Ordering::Acquire)
    }

    /// 检查音符实例是否为空
    pub fn note_instances_is_empty(&self) -> bool {
        self.note_instances_len() == 0
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
