//! GPU 音符缓冲区类型定义

use crate::epoch::GpuEpoch;
use crate::gpu_resource_tracker::TrackedBuffer;

/// GPU 音符缓冲区
pub struct GpuNoteBuffer {
    /// 实例缓冲区（常驻 GPU 内存）
    pub(crate) instance_buffer: TrackedBuffer,
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
    /// 创建时所属 GPU 代次（REND-019 PR-B：跨帧常驻缓冲的登记基准；
    /// 跨代次判废走既有重建路径——随宿主渲染器 / 线程整体重建，E2/E3）
    pub(crate) epoch: GpuEpoch,
}

impl GpuNoteBuffer {
    /// 创建时所属 GPU 代次（REND-019 PR-B：跨代次判废的登记读取点）。
    #[must_use]
    pub fn gpu_epoch(&self) -> GpuEpoch {
        self.epoch
    }
}
