use iced_wgpu::wgpu;

/// 渲染缓存 - 避免每帧重复上传相同数据
pub struct RenderCache {
    /// 缓存的网格线实例
    pub grid_instances: Vec<lumino_gfx::GridLineInstance>,
    /// 缓存的音符实例
    pub note_instances: Vec<lumino_gfx::NoteInstance>,
    /// 网格线视口哈希（用于检测变化）
    pub grid_viewport_hash: u64,
    /// 音符视口哈希（用于检测变化）
    pub note_viewport_hash: u64,
    /// 缓存的深度纹理 (宽, 高, view)
    pub depth_texture: Option<(u32, u32, wgpu::TextureView)>,
}

impl Default for RenderCache {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderCache {
    pub fn new() -> Self {
        Self {
            grid_instances: Vec::new(),
            note_instances: Vec::new(),
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
    ) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        scroll_x.to_bits().hash(&mut hasher);
        scroll_y.to_bits().hash(&mut hasher);
        zoom_x.to_bits().hash(&mut hasher);
        zoom_y.to_bits().hash(&mut hasher);
        canvas_width.to_bits().hash(&mut hasher);
        canvas_height.to_bits().hash(&mut hasher);
        hasher.finish()
    }
}
