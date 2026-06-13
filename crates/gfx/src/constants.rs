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

    /// 网格渲染常量
    pub mod grid {
        /// 默认每小节 tick 数 (1920)
        pub const TICKS_PER_MEASURE: u32 = 1920;
        /// 默认每拍 tick 数 (480)
        pub const TICKS_PER_BEAT: u32 = 480;

        /// 默认网格颜色（独立线程渲染路径）
        pub mod colors {
            pub const BLACK_KEY_LINE: [f32; 4] = [0.15, 0.15, 0.15, 1.0];
            pub const WHITE_KEY_LINE: [f32; 4] = [0.1, 0.1, 0.1, 1.0];
            pub const BAR_LINE: [f32; 4] = [0.3, 0.3, 0.3, 1.0];
            pub const BEAT_LINE: [f32; 4] = [0.2, 0.2, 0.2, 1.0];
            pub const HALF_BEAT_LINE: [f32; 4] = [0.2, 0.2, 0.2, 0.5];
            pub const GRID_LINE: [f32; 4] = [0.2, 0.2, 0.2, 0.2];
        }
    }

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
