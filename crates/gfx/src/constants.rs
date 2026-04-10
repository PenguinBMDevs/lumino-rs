//! GFX 模块常量

/// 渲染相关常量
pub mod rendering {
    /// 初始实例缓冲区容量（黑乐谱场景需要更大预分配）
    pub const INITIAL_INSTANCE_CAPACITY: usize = 65536;
    /// 实例缓冲区扩容因子
    pub const BUFFER_GROWTH_FACTOR: usize = 2;
    /// 单次最大上传实例数（防止一次性上传过多导致帧卡顿）
    pub const MAX_UPLOAD_PER_FRAME: usize = 500_000;
}
