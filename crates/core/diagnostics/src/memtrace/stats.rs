use super::allocator::COUNTERS;
use super::*;

use std::sync::atomic::{AtomicIsize, Ordering};

/// Tracks GPU resource memory that does not go through the Rust global
/// allocator (e.g. wgpu textures/buffers allocated by the graphics driver).
static GPU_RESOURCE_BYTES: AtomicIsize = AtomicIsize::new(0);

/// Add `bytes` to the GPU resource counter. Called when a wgpu Texture or
/// Buffer is created.
pub fn add_gpu_resource(bytes: u64) {
    GPU_RESOURCE_BYTES.fetch_add(bytes as isize, Ordering::Relaxed);
}

/// Subtract `bytes` from the GPU resource counter. Called when a wgpu Texture
/// or Buffer is dropped/replaced.
pub fn sub_gpu_resource(bytes: u64) {
    GPU_RESOURCE_BYTES.fetch_sub(bytes as isize, Ordering::Relaxed);
}

/// Current GPU resource memory in bytes.
pub fn gpu_resource_bytes() -> isize {
    GPU_RESOURCE_BYTES.load(Ordering::Relaxed)
}

/// Current GPU resource memory in megabytes.
pub fn gpu_resource_mb() -> f64 {
    gpu_resource_bytes() as f64 / 1_048_576.0
}

impl Snapshot {
    /// 捕获当前所有内存计数器的快照。
    ///
    /// 读取各分配标签的原子计数器与 GPU 资源计数器。
    ///
    /// # 返回值
    /// 返回代表当前内存状态的一份 `Snapshot`
    pub fn capture() -> Self {
        let mut bytes = [0; AllocTag::COUNT];
        for (i, counter) in COUNTERS.iter().enumerate() {
            bytes[i] = counter.load(Ordering::Relaxed);
        }
        Self {
            bytes,
            gpu_resources: gpu_resource_bytes(),
        }
    }

    /// 返回指定分配标签已追踪的内存字节数。
    ///
    /// # 参数
    /// * `tag` — 目标分配标签
    ///
    /// # 返回值
    /// 该标签对应的已追踪字节数（可为负，表示释放多于分配的异常情况）
    pub fn get(&self, tag: AllocTag) -> isize {
        self.bytes[tag as usize]
    }

    /// 返回全部分配标签的已追踪字节数总和。
    ///
    /// # 返回值
    /// 各标签已追踪字节数之和（不含 GPU 资源）
    pub fn total_tracked(&self) -> isize {
        self.bytes.iter().sum()
    }

    /// Total tracked memory including GPU resources.
    pub fn total_with_gpu(&self) -> isize {
        self.total_tracked().saturating_add(self.gpu_resources)
    }

    /// 返回指定 tag 当前追踪的内存用量（单位：MB）。
    pub fn tracked_mb(&self, tag: AllocTag) -> f64 {
        self.get(tag) as f64 / 1_048_576.0
    }

    /// 返回所有已追踪内存用量（不含 GPU 资源），单位为 MB。
    ///
    /// # 返回值
    /// 已追踪内存总量的兆字节数
    pub fn total_mb(&self) -> f64 {
        self.total_tracked() as f64 / 1_048_576.0
    }

    /// 返回含 GPU 资源的总内存用量，单位为 MB。
    ///
    /// # 返回值
    /// 已追踪内存与 GPU 资源合计的兆字节数
    pub fn total_with_gpu_mb(&self) -> f64 {
        self.total_with_gpu() as f64 / 1_048_576.0
    }

    /// 返回 GPU 资源内存用量，单位为 MB。
    ///
    /// # 返回值
    /// GPU 资源字节数折算的兆字节数
    pub fn gpu_mb(&self) -> f64 {
        self.gpu_resources as f64 / 1_048_576.0
    }
}
