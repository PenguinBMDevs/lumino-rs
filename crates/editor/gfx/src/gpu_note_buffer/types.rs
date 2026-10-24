//! GPU 音符缓冲区类型定义
//!
//! 2026-10-17 单一权威源整改：删除 `instances: Vec<NoteInstance>` CPU 全量镜像。
//!
//! 此前 `upload_all` 会把整份实例额外复制进一张 CPU `Vec` 供增量路径索引
//! （6 亿音符实测 9.6 GB，CPU 峰值 30–40 GB）。GPU 常驻缓冲才是音符实例的唯一
//! 权威，CPU 不得再持有影子副本：
//! - 全量/流式上传都只写 GPU（`upload_all` / `begin_streaming_upload`）
//! - 所有增量（`update_note(s)` / `remove_at` / `insert_at` / `remove_note`）
//!   只改 GPU 字节，其中保序删/插走 GPU 内部搬移，swap-remove 走 GPU 内 16B 拷贝

use crate::epoch::GpuEpoch;
use crate::gpu_resource_tracker::TrackedBuffer;

/// GPU 音符缓冲区
pub struct GpuNoteBuffer {
    /// 实例缓冲区（常驻 GPU 内存，音符实例的唯一权威存储）
    pub(crate) instance_buffer: TrackedBuffer,
    /// 当前缓冲区容量（实例数量）
    pub(crate) capacity: usize,
    /// 当前实际存储的实例数量
    pub(crate) instance_count: usize,
    /// 最大容量限制（`max_storage_buffer_binding_size / 16`，硬件物理上限）
    pub(crate) max_capacity: usize,
    /// 本次流式会话因超过 `max_capacity` 被丢弃的实例数（仅诊断；结束时报一次）
    pub(crate) dropped_instances: usize,
    /// 设备引用（用于扩容）
    pub(crate) device: std::sync::Arc<wgpu::Device>,
    /// 队列引用（用于更新）
    pub(crate) queue: std::sync::Arc<wgpu::Queue>,
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
