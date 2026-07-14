/// 单张贴图的 GPU 资源
///
/// `texture` 和 `view` 虽不直接读取，但必须保活以持有 GPU 资源所有权，
/// drop 时自动释放显存。
#[allow(dead_code)]
pub(super) struct TileGpuResource {
    pub(super) texture: wgpu::Texture,
    pub(super) view: wgpu::TextureView,
    pub(super) bind_group: wgpu::BindGroup,
    pub(super) uniform_buffer: wgpu::Buffer,
    pub(super) byte_size: usize,
}
