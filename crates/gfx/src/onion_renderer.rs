//! 洋葱皮 GPU 渲染管线 — 参考 Wasabi 瀑布流实现的简化版本
//!
//! 移除了旧版的 compute shader 可见性剔除、间接绘制、bucket 模式等复杂逻辑。
//! 新的渲染方案：
//!
//! 数据流:
//!   1. 所有洋葱皮音符作为 OnionNote 数组上传到 GPU storage buffer
//!   2. 每帧更新视口 uniform（OnionViewportUniform）
//!   3. 使用 TriangleStrip + 4 顶点/实例绘制所有音符
//!   4. Vertex shader 读取 OnionNote 计算 NDC 坐标
//!   5. GPU 自动裁剪超出 [-1, 1] 的音符（无需 CPU 或 compute 预处理）
//!
//! 方向：钢琴卷帘方向（X=time, Y=pitch），参考 Wasabi 瀑布流旋转 90°
//!
//! 参考 Wasabi 文件：
//! - gui/window/scene/note_list_system/mod.rs (NoteRenderer::draw)
//! - shaders/notes/notes.geom (几何着色器展开点→矩形)
//! - shaders/notes/notes.frag (片段着色器)
//!
//! 因为 WGSL 不支持 geometry shader，改用 instanced rendering 等效实现

pub mod types;

pub use types::{OnionNote, OnionViewportUniform};

mod buffer;
mod init;
mod upload;

/// 洋葱皮渲染器（简化版 — 无 compute cull，无 indirect draw）
pub struct OnionRenderer {
    // ─── GPU 资源 ──────────────────────────────────────
    /// 音符池 Storage Buffer（所有洋葱皮音符，SoA 布局）
    note_pool_buffer: wgpu::Buffer,
    /// 视口 uniform buffer
    viewport_buffer: wgpu::Buffer,

    // ─── Pipeline ──────────────────────────────────────
    render_pipeline: wgpu::RenderPipeline,
    render_bind_group: wgpu::BindGroup,
    render_bind_group_layout: wgpu::BindGroupLayout,

    // ─── 状态 ──────────────────────────────────────────
    /// 音符池容量（OnionNote 数量）
    note_pool_capacity: usize,
    /// 实际音符数量（准备渲染）
    note_count: u32,
    /// GPU max storage buffer binding size
    max_storage_binding: u64,
    /// CPU 侧缓存的上色音符（跨帧复用，避免每帧分配）
    cpu_note_pool: Vec<OnionNote>,
    /// 上一次上传的 note list 版本号（dirty tracking）
    last_list_version: u64,
    /// 上一次上传的颜色哈希值（dirty tracking）
    last_color_hash: u64,
}

impl OnionRenderer {
    /// 初始音符池容量（起步 8K，按需扩容）
    const INITIAL_NOTE_CAPACITY: usize = 8192;
    /// 最大音符池容量（~512 MB for OnionNote@16B）
    const MAX_NOTE_POOL_CAPACITY: usize = 33_554_432;

    /// 着色器源码
    const SHADER_SRC: &'static str = include_str!("./shaders/onion_render.wgsl");

    /// 获取当前音符数量
    pub fn note_count(&self) -> u32 {
        self.note_count
    }
}
