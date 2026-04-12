//! GFX 模块常量

/// 渲染相关常量
pub mod rendering {
    /// 初始实例缓冲区容量（黑乐谱场景需要更大预分配）
    pub const INITIAL_INSTANCE_CAPACITY: usize = 65536;
    /// 实例缓冲区扩容因子
    pub const BUFFER_GROWTH_FACTOR: usize = 2;
    /// 单次最大上传实例数（防止一次性上传过多导致帧卡顿）
    pub const MAX_UPLOAD_PER_FRAME: usize = 500_000;
    /// 深度纹理格式
    pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    /// 标准深度/模板状态
    pub fn depth_stencil_state() -> Option<wgpu::DepthStencilState> {
        Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        })
    }
}
