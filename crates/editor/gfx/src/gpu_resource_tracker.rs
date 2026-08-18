//! GPU 资源内存追踪辅助函数
//!
//! 将 wgpu Texture / Buffer 的创建与释放上报到 lumino_diagnostics::memtrace。

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

/// 上报即将释放的 Texture。
pub fn sub_texture(texture: &wgpu::Texture) {
    lumino_diagnostics::memtrace::sub_gpu_resource(texture_size_bytes(texture));
}

/// 上报新创建的 Buffer。
pub fn add_buffer(buffer: &wgpu::Buffer) {
    lumino_diagnostics::memtrace::add_gpu_resource(buffer.size());
}

/// 上报即将释放的 Buffer。
pub fn sub_buffer(buffer: &wgpu::Buffer) {
    lumino_diagnostics::memtrace::sub_gpu_resource(buffer.size());
}
