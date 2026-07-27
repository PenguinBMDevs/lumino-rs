//! Comet 风格 GPU 渲染器共享类型
//!
//! 所有 Comet 视频导出样式（Enhanced、MIDITrail、PFA、Velocities、Channels）
//! 使用统一的音符输入格式，由 `CometRenderer` 根据样式分派到不同计算着色器。

/// Comet 样式枚举（与 `lumino_event::window::video::RenderMode` 对齐，但仅含 Comet 相关）
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CometRenderStyle {
    /// Enhanced 3D + Bloom 风格
    Enhanced = 0,
    /// MIDITrail 轨迹拖影 + 3D 键盘风格
    MIDITrail = 1,
    /// PFA 侧视图钢琴卷帘风格
    PFA = 2,
    /// Velocities 力度热力图风格
    Velocities = 3,
    /// Channels 通道热力图风格
    Channels = 4,
}

impl CometRenderStyle {
    /// 着色器入口函数名。
    pub const fn entry_point(self) -> &'static str {
        match self {
            Self::Enhanced => "enhanced_main",
            Self::MIDITrail => "miditrail_main",
            Self::PFA => "pfa_main",
            Self::Velocities => "velocities_main",
            Self::Channels => "channels_main",
        }
    }
}

/// 单个音符的 GPU 表示（32 字节，对齐友好）
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CometNoteGpu {
    /// MIDI 键号（0-255）
    pub key: u32,
    /// 音符起始 tick
    pub start_tick: u32,
    /// 音符结束 tick
    pub end_tick: u32,
    /// 打包 BGRA 颜色（与 WaterfallNoteGpu 兼容）
    pub color_packed: u32,
    /// 音轨索引（用于调色板 / 通道）
    pub track_idx: u32,
    /// 音符力度（0-127）
    pub velocity: u32,
    /// MIDI 通道（0-15，如未解析可填 track_idx % 16）
    pub channel: u32,
    /// 对齐填充
    pub _padding: u32,
}

/// 每帧 Uniform 参数（48 字节，16 字节对齐）
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CometUniformGpu {
    /// 当前播放 tick
    pub tick: u32,
    /// 每四分音符 tick 数
    pub ppq: u32,
    /// 键盘键数（通常为 128）
    pub key_count: u32,
    /// 帧宽度（像素）
    pub frame_width: u32,
    /// 帧高度（像素）
    pub frame_height: u32,
    /// 键盘高度（像素）
    pub kb_height: u32,
    /// 当前样式（对应 CometRenderStyle）
    pub style: u32,
    /// 滚动速度倍率
    pub speed: f32,
    /// 样式附加参数 1
    pub param1: f32,
    /// 样式附加参数 2
    pub param2: f32,
    /// 对齐填充
    pub _padding0: u32,
    pub _padding1: u32,
}
