// 重新导出相机 uniform（复用 note_renderer 的 CameraUniform）
pub use crate::note_renderer::CameraUniform;

/// SoA 布局的洋葱皮音符 — 16 字节对齐
///
/// 字段平铺为 Structure of Arrays 形式常驻 GPU storage buffer：
/// - start_tick / end_tick: tick 范围（u32，单位 tick）
/// - pitch: 音高 (u8)
/// - track_idx: 音轨索引 (u16)
/// - color_packed: RGBA8 颜色（替代 uniform 查表，支持任意数量音轨）
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct OnionNote {
    /// 起始 tick
    pub start_tick: u32,
    /// 结束 tick
    pub end_tick: u32,
    /// 低 8 位 = pitch，位 8-23 = track_idx，高 8 位保留
    pub packed: u32,
    /// RGBA8 打包颜色（r << 24 | g << 16 | b << 8 | a），替代 uniform 颜色表
    pub color_packed: u32,
}

impl OnionNote {
    pub fn new(start_tick: u32, end_tick: u32, pitch: u8, track_idx: u16) -> Self {
        Self {
            start_tick,
            end_tick,
            packed: (track_idx as u32) << 8 | pitch as u32,
            color_packed: 0, // 默认透明黑，调用方负责设置
        }
    }

    pub fn new_with_color(
        start_tick: u32,
        end_tick: u32,
        pitch: u8,
        track_idx: u16,
        color_packed: u32,
    ) -> Self {
        Self {
            start_tick,
            end_tick,
            packed: (track_idx as u32) << 8 | pitch as u32,
            color_packed,
        }
    }

    pub fn pitch(&self) -> u8 {
        (self.packed & 0xFF) as u8
    }

    pub fn track_idx(&self) -> u16 {
        ((self.packed >> 8) & 0xFFFF) as u16
    }

    /// 设置 RGBA8 打包颜色
    pub fn set_color_packed(&mut self, rgba: u32) {
        self.color_packed = rgba;
    }

    /// 获取 RGBA8 打包颜色
    pub fn color_packed(&self) -> u32 {
        self.color_packed
    }

    /// 从 RGBA f32 分量打包为 u32
    pub fn pack_rgba(r: f32, g: f32, b: f32, a: f32) -> u32 {
        let ri = (r.clamp(0.0, 1.0) * 255.0) as u32;
        let gi = (g.clamp(0.0, 1.0) * 255.0) as u32;
        let bi = (b.clamp(0.0, 1.0) * 255.0) as u32;
        let ai = (a.clamp(0.0, 1.0) * 255.0) as u32;
        (ri << 24) | (gi << 16) | (bi << 8) | ai
    }
}

/// 单个 key 在 GPU 音符池中的可见扫描范围
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct OnionKeyRange {
    /// 起始索引（含）
    pub start: u32,
    /// 结束索引（不含）
    pub end: u32,
}

/// 视口裁剪 uniform — 定义可见 tick/pitch 范围 + cull 参数
///
/// 两种工作模式：
/// 1. 兼容模式（use_key_ranges == 0）：GPU 在 [visible_start, visible_end) 区间内做全量裁剪。
/// 2. Bucket 模式（use_key_ranges != 0）：GPU 通过 `OnionKeyRange` 缓冲区只扫描每个 key 的
///    可见子区间，避免扫描整个音符池。
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct OnionViewportUniform {
    /// 可见 tick 范围 [start, end)
    pub tick_start: f32,
    pub tick_end: f32,
    /// 可见 pitch 范围 [min, max]
    pub pitch_min: f32,
    pub pitch_max: f32,
    /// 音符总数
    pub note_count: u32,
    /// 实例索引缓冲区容量
    pub indices_capacity: u32,
    /// 当前编辑音轨（GPU 剔除时排除）
    pub current_track: u32,
    /// 0=兼容模式全量扫描；1=使用 per-key 范围缓冲区
    pub use_key_ranges: u32,
    /// GPU 扫描区间 [visible_start, visible_end)（兼容模式使用）
    pub visible_start: u32,
    pub visible_end: u32,
}

impl Default for OnionViewportUniform {
    fn default() -> Self {
        Self {
            tick_start: 0.0,
            tick_end: 0.0,
            pitch_min: 0.0,
            pitch_max: 0.0,
            note_count: 0,
            indices_capacity: 65536,
            current_track: 0,
            use_key_ranges: 0,
            visible_start: 0,
            visible_end: 0,
        }
    }
}

/// 轨道掩码 — 支持最多 64 个轨道（CPU 端过滤用）
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct OnionTrackMask {
    /// 低 32 位
    pub mask_lo: u32,
    /// 高 32 位
    pub mask_hi: u32,
}

impl OnionTrackMask {
    pub fn new(track_indices: &[u16]) -> Self {
        let mut lo = 0u64;
        for &idx in track_indices {
            lo |= 1u64 << idx;
        }
        Self {
            mask_lo: lo as u32,
            mask_hi: (lo >> 32) as u32,
        }
    }

    pub fn all() -> Self {
        Self {
            mask_lo: u32::MAX,
            mask_hi: u32::MAX,
        }
    }

    pub fn empty() -> Self {
        Self {
            mask_lo: 0,
            mask_hi: 0,
        }
    }

    pub fn set(&mut self, track_idx: u16, visible: bool) {
        if track_idx >= 64 {
            return;
        }
        let mask = 1u64 << track_idx;
        let current = (self.mask_hi as u64) << 32 | self.mask_lo as u64;
        let new = if visible {
            current | mask
        } else {
            current & !mask
        };
        self.mask_lo = new as u32;
        self.mask_hi = (new >> 32) as u32;
    }

    /// 检查指定音轨是否可见
    pub fn is_track_visible(&self, track_idx: u16) -> bool {
        if track_idx >= 64 {
            return false;
        }
        let mask = 1u64 << track_idx;
        let current = (self.mask_hi as u64) << 32 | self.mask_lo as u64;
        (current & mask) != 0
    }
}

/// 间接绘制参数 — 匹配 VkDrawIndirectCommand（16 字节）
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DrawIndirectArgs {
    pub vertex_count: u32,
    pub instance_count: u32,
    pub first_vertex: u32,
    pub first_instance: u32,
}

impl Default for DrawIndirectArgs {
    fn default() -> Self {
        Self {
            vertex_count: 4,
            instance_count: 0,
            first_vertex: 0,
            first_instance: 0,
        }
    }
}
