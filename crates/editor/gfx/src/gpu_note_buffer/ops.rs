//! GPU 音符缓冲区公开操作

mod incremental;
mod move_blocks;
mod update;
mod upload;

use super::types::GpuNoteBuffer;

impl GpuNoteBuffer {
    /// 获取实例缓冲区引用（用于渲染）
    pub fn buffer(&self) -> &wgpu::Buffer {
        self.instance_buffer.inner()
    }

    /// 获取当前实例数量
    pub fn len(&self) -> usize {
        self.instance_count
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.instance_count == 0
    }

    /// 获取当前容量
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// 获取 GPU 内存占用（字节）
    pub fn gpu_memory_usage(&self) -> usize {
        self.instance_buffer.size() as usize
    }

    /// 共享 CPU 镜像只读视图（供渲染线程派生计算复用，零第二份 CPU 拷贝）。
    ///
    /// 不变式：镜像仅在 `upload_all` 路径维护；流式上传（`begin_streaming_upload`）
    /// 会清空镜像，调用方须保证此前走的是全量上传。
    pub fn shared_cpu_instances(&self) -> &[crate::NoteInstance] {
        &self.instances
    }
}

#[cfg(test)]
#[path = "ops_tests.rs"]
mod tests;
