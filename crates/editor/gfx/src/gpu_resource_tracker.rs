//! GPU 资源内存追踪辅助函数
//!
//! 将 wgpu Texture / Buffer 的创建与释放上报到 lumino_diagnostics::memtrace。
//!
//! 推荐用法：字段级持有的资源使用 [`TrackedBuffer`] / [`TrackedTexture`] 包装，
//! `Drop` 自动注销，杜绝漏调用导致的统计偏差。

use wgpu::util::DeviceExt;

/// 计算纹理占用的字节数（含 block-compressed 格式）。
pub fn texture_size_bytes(texture: &wgpu::Texture) -> u64 {
    texture_extent_size_bytes(texture.format(), texture.size())
}

/// 根据格式与尺寸计算纹理字节数。
pub fn texture_extent_size_bytes(format: wgpu::TextureFormat, size: wgpu::Extent3d) -> u64 {
    let (block_w, block_h) = format.block_dimensions();
    // 对未知/特殊格式（如 Depth24Plus）回退到 4 字节近似值
    let block_size = format.block_copy_size(None).unwrap_or(4) as u64;
    let width_blocks = (size.width as u64).div_ceil(block_w as u64);
    let height_blocks = (size.height as u64).div_ceil(block_h as u64);
    width_blocks * height_blocks * size.depth_or_array_layers as u64 * block_size
}

/// 上报新创建的 Texture。
pub fn add_texture(texture: &wgpu::Texture) {
    lumino_diagnostics::memtrace::add_gpu_resource(texture_size_bytes(texture));
}

/// 上报新创建的 Buffer。
pub fn add_buffer(buffer: &wgpu::Buffer) {
    lumino_diagnostics::memtrace::add_gpu_resource(buffer.size());
}

/// 创建实例缓冲区并自动上报资源（消除各 renderer 重复的创建 + add_buffer 样板）。
///
/// 统一用法：`Vertex | CopyDst` 用途、`mapped_at_creation: false`。
/// 返回 [`TrackedBuffer`]：扩容重建时旧缓冲自动注销，无需手动 `sub_buffer`。
pub fn create_instance_buffer<T: bytemuck::Pod>(
    device: &wgpu::Device,
    label: &'static str,
    capacity: usize,
) -> TrackedBuffer {
    TrackedBuffer::new(
        device,
        &wgpu::BufferDescriptor {
            label: Some(label),
            size: (capacity * std::mem::size_of::<T>()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        },
    )
}

/// 自动追踪的 Buffer 包装器（文档三 §2.3）
///
/// 创建时自动上报内存，`Drop` 时自动注销——杜绝手动 `sub_buffer` 漏调用
/// 导致的内存统计偏差。持有者通过 [`TrackedBuffer::inner`] 访问 wgpu 资源。
pub struct TrackedBuffer {
    buffer: wgpu::Buffer,
    size: u64,
}

impl TrackedBuffer {
    /// 创建缓冲区并自动上报资源占用。
    pub fn new(device: &wgpu::Device, desc: &wgpu::BufferDescriptor) -> Self {
        let buffer = device.create_buffer(desc);
        let size = buffer.size();
        add_buffer(&buffer);
        Self { buffer, size }
    }

    /// 创建带初始数据的缓冲区（等价 `wgpu::util::DeviceExt::create_buffer_init`）并自动上报。
    pub fn new_init(device: &wgpu::Device, desc: &wgpu::util::BufferInitDescriptor) -> Self {
        let buffer = device.create_buffer_init(desc);
        let size = buffer.size();
        add_buffer(&buffer);
        Self { buffer, size }
    }

    /// 访问内部 wgpu 缓冲区。
    #[inline]
    pub fn inner(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    /// 缓冲区大小（字节）。
    #[inline]
    pub fn size(&self) -> u64 {
        self.size
    }
}

impl Drop for TrackedBuffer {
    fn drop(&mut self) {
        lumino_diagnostics::memtrace::sub_gpu_resource(self.size);
    }
}

/// 自动追踪的 Texture 包装器（文档三 §2.3）
///
/// 创建时自动上报内存，`Drop` 时自动注销。持有者通过 [`TrackedTexture::inner`]
/// 访问 wgpu 资源。
pub struct TrackedTexture {
    texture: wgpu::Texture,
    size: u64,
}

impl TrackedTexture {
    /// 创建纹理并自动上报资源占用。
    pub fn new(device: &wgpu::Device, desc: &wgpu::TextureDescriptor) -> Self {
        let texture = device.create_texture(desc);
        let size = texture_size_bytes(&texture);
        add_texture(&texture);
        Self { texture, size }
    }

    /// 访问内部 wgpu 纹理。
    #[inline]
    pub fn inner(&self) -> &wgpu::Texture {
        &self.texture
    }

    /// 创建纹理视图。
    #[inline]
    pub fn create_view(&self, desc: &wgpu::TextureViewDescriptor) -> wgpu::TextureView {
        self.texture.create_view(desc)
    }
}

impl Drop for TrackedTexture {
    fn drop(&mut self) {
        lumino_diagnostics::memtrace::sub_gpu_resource(self.size);
    }
}
