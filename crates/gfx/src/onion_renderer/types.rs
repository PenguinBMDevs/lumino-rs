// 重新导出相机 uniform（复用 note_renderer 的 CameraUniform）
pub use crate::note_renderer::CameraUniform;

/// SoA 布局的洋葱皮音符 — 16 字节对齐
///
/// 字段平铺为 Structure of Arrays 形式常驻 GPU storage buffer：
/// - start_tick / end_tick: tick 范围（u32，单位 tick）
/// - pitch: 音高 (u8)
/// - track_idx: 音轨索引 (u16)
/// - 填充至 16 字节满足 storage buffer 对齐要求
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct OnionNote {
    /// 起始 tick
    pub start_tick: u32,
    /// 结束 tick
    pub end_tick: u32,
    /// 低 8 位 = pitch，位 8-23 = track_idx，高 8 位保留
    pub packed: u32,
    /// 填充至 16 字节
    pub _padding: u32,
}

impl OnionNote {
    pub fn new(start_tick: u32, end_tick: u32, pitch: u8, track_idx: u16) -> Self {
        Self {
            start_tick,
            end_tick,
            packed: (track_idx as u32) << 8 | pitch as u32,
            _padding: 0,
        }
    }

    pub fn pitch(&self) -> u8 {
        (self.packed & 0xFF) as u8
    }

    pub fn track_idx(&self) -> u16 {
        ((self.packed >> 8) & 0xFFFF) as u16
    }
}

/// 视口裁剪 uniform — 定义可见 tick/pitch 范围 + cull 参数
///
/// 音符池需按 start_tick 升序排列，fill_cull_range 在 CPU 端执行二分查找
/// 定位可见范围 [visible_start, visible_end)，GPU 仅扫描该区间
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
    /// CPU 二分查找定位的可见区间 [visible_start, visible_end)
    /// 仅该区间内的音符会进入 cull shader 处理
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
            visible_start: 0,
            visible_end: 0,
        }
    }
}

impl OnionViewportUniform {
    /// CPU 二分查找填充 visible_start/visible_end
    ///
    /// 前提：notes 已按 start_tick 升序排列
    pub fn fill_cull_range(&mut self, notes: &[OnionNote]) {
        let tick_start_u = self.tick_start as u32;
        let tick_end_u = self.tick_end as u32;

        // 二分查找到第一个 start_tick > tick_end 的位置
        let end = notes.partition_point(|n| n.start_tick <= tick_end_u);

        // 二分查找第一个 start_tick >= tick_start 的位置，回退 256 作为安全余量
        let start_bin = notes[..end].partition_point(|n| n.start_tick < tick_start_u);
        let start = start_bin.saturating_sub(256);

        self.visible_start = start as u32;
        self.visible_end = end as u32;
    }
}

/// 轨道掩码 uniform — 支持最多 64 个轨道
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
}

/// 轨道颜色 uniform — 固定 64 条轨道，每条 16 字节对齐
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct OnionTrackColors {
    pub colors: [TrackColor; 64],
}

impl Default for OnionTrackColors {
    fn default() -> Self {
        Self {
            colors: [TrackColor::default(); 64],
        }
    }
}

/// 单条轨道颜色 — vec4<f32> 对齐
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TrackColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Default for TrackColor {
    fn default() -> Self {
        Self {
            r: 0.5,
            g: 0.5,
            b: 0.5,
            a: 0.3,
        }
    }
}

impl TrackColor {
    pub fn from_rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
}

/// 间接绘制参数 — 匹配 VkDrawIndexedIndirectCommand（16 字节）
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
