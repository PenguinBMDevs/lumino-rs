//! GPU 音符缓冲区类型定义

/// GPU 音符缓冲区
pub struct GpuNoteBuffer {
    /// 实例缓冲区（常驻 GPU 内存）
    pub(crate) instance_buffer: wgpu::Buffer,
    /// 当前缓冲区容量（实例数量）
    pub(crate) capacity: usize,
    /// 当前实际存储的实例数量
    pub(crate) instance_count: usize,
    /// 最大容量限制
    pub(crate) max_capacity: usize,
    /// 设备引用（用于扩容）
    pub(crate) device: std::sync::Arc<wgpu::Device>,
    /// 队列引用（用于更新）
    pub(crate) queue: std::sync::Arc<wgpu::Queue>,
    /// CPU 侧实例缓存（用于支持删除等操作）
    pub(crate) instances: Vec<crate::NoteInstance>,
}
