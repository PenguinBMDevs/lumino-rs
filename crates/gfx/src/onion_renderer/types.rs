//! 洋葱皮渲染类型定义 — GPU compute cull + indirect draw 类型系统

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

/// 视口 uniform — 传递给 vertex shader 用于坐标变换
///
/// 参考 Wasabi 的 push constants 设计，但通过 uniform buffer 传递。
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
    /// 当前编辑音轨（vertex shader 剔除时排除）
    pub current_track: u32,
    /// 视口变换参数（像素坐标 → NDC）
    pub keyboard_width: f32,
    pub ruler_height: f32,
    pub canvas_width: f32,
    pub canvas_height: f32,
    pub canvas_offset_x: f32,
    pub canvas_offset_y: f32,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub zoom_x: f32,
    pub zoom_y: f32,
    pub max_key_index: f32,
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

impl Default for OnionViewportUniform {
    fn default() -> Self {
        Self {
            tick_start: 0.0,
            tick_end: 0.0,
            pitch_min: 0.0,
            pitch_max: 0.0,
            note_count: 0,
            current_track: 0,
            keyboard_width: 0.0,
            ruler_height: 0.0,
            canvas_width: 800.0,
            canvas_height: 600.0,
            canvas_offset_x: 0.0,
            canvas_offset_y: 0.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
            zoom_x: 0.1,
            zoom_y: 20.0,
            max_key_index: 127.0,
        }
    }
}
