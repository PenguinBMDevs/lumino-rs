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
}

#[cfg(test)]
#[path = "ops_tests.rs"]
mod tests;
