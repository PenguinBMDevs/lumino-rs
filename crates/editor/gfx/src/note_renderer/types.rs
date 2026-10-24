//! 音符渲染器类型定义

/// 预览音符的 border_width 哨兵值。
/// 当 `border_width == PREVIEW_BORDER_SENTINEL` 时，FS 走预览分支（70% alpha，不画边框）。
pub const PREVIEW_BORDER_SENTINEL: u32 = 0xFFFF_FFFF;

/// 音符逻辑实例数据 — **12 bytes**（性能模式）。
///
/// 16B 的 wasabi 布局已删除。12B 的取舍：
///   - `start_length` 仍为 `[f32;2]`，与旧 16B 逐位同精度；
///   - `attrs` 合并 `key(8) | track_enc(16) | border(7) | preview(1)`；
///   - 颜色不再逐实例存 RGB：生产颜色 = 轨道调色板色，主轨固定蓝。
///
/// 这样 310M 实例约 3.73GB，单 buffer 可容纳（仍需按 2GB binding 分 chunk）。
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct NoteInstance {
    /// `[start_tick, length_tick]`
    pub start_length: [f32; 2],
    /// key(0..7) | track_enc(8..23) | border(24..30) | preview(31)
    pub attrs: u32,
}

impl NoteInstance {
    /// attrs 中各字段的位宽/掩码。
    pub const KEY_MASK: u32 = 0xFF;
    pub const TRACK_SHIFT: u32 = 8;
    pub const TRACK_MASK: u32 = 0xFFFF;
    pub const BORDER_SHIFT: u32 = 24;
    pub const BORDER_MASK: u32 = 0x7F;
    pub const PREVIEW_BIT: u32 = 0x8000_0000;

    #[must_use]
    pub fn key(&self) -> u8 {
        (self.attrs & Self::KEY_MASK) as u8
    }

    /// 轨道编码（track_idx+1；0 = 主轨/preview 的保留编码）
    #[must_use]
    pub fn track_enc(&self) -> u16 {
        ((self.attrs >> Self::TRACK_SHIFT) & Self::TRACK_MASK) as u16
    }

    /// 边框像素宽（0..127；生产值 0/1/2/4/6/8/10）
    #[must_use]
    pub fn border_width(&self) -> u8 {
        ((self.attrs >> Self::BORDER_SHIFT) & Self::BORDER_MASK) as u8
    }

    #[must_use]
    pub fn is_preview(&self) -> bool {
        self.attrs & Self::PREVIEW_BIT != 0
    }

    pub(crate) fn encode_attrs(key: u8, track_enc: u16, border: u8, preview: bool) -> u32 {
        (key as u32)
            | ((track_enc as u32) << Self::TRACK_SHIFT)
            | (((border as u32) & Self::BORDER_MASK) << Self::BORDER_SHIFT)
            | if preview { Self::PREVIEW_BIT } else { 0 }
    }
}

/// 旧调色板数据的硬编码备用色（与 `lumino_extras::palette::FALLBACK_PALETTE` 一致）。
///
/// 12B 实例不存 RGB；GPU 侧由调色板缓冲查色，CPU 侧（miditrail/视频导出辅助
/// 路径）在尚未接入 UI 调色板时使用同一份 fallback，保证颜色不死黑。
pub const FALLBACK_TRACK_COLORS_U8: [[u8; 3]; 12] = [
    [200, 80, 80],
    [80, 200, 120],
    [80, 120, 220],
    [220, 200, 80],
    [200, 100, 200],
    [80, 200, 200],
    [240, 150, 80],
    [180, 180, 180],
    [230, 100, 100],
    [100, 180, 100],
    [100, 100, 200],
    [200, 180, 100],
];

pub const FALLBACK_TRACK_COLORS: [[f32; 4]; 12] = [
    [200.0 / 255.0, 80.0 / 255.0, 80.0 / 255.0, 1.0],
    [80.0 / 255.0, 200.0 / 255.0, 120.0 / 255.0, 1.0],
    [80.0 / 255.0, 120.0 / 255.0, 220.0 / 255.0, 1.0],
    [220.0 / 255.0, 200.0 / 255.0, 80.0 / 255.0, 1.0],
    [200.0 / 255.0, 100.0 / 255.0, 200.0 / 255.0, 1.0],
    [80.0 / 255.0, 200.0 / 255.0, 200.0 / 255.0, 1.0],
    [240.0 / 255.0, 150.0 / 255.0, 80.0 / 255.0, 1.0],
    [180.0 / 255.0, 180.0 / 255.0, 180.0 / 255.0, 1.0],
    [230.0 / 255.0, 100.0 / 255.0, 100.0 / 255.0, 1.0],
    [100.0 / 255.0, 180.0 / 255.0, 100.0 / 255.0, 1.0],
    [100.0 / 255.0, 100.0 / 255.0, 200.0 / 255.0, 1.0],
    [200.0 / 255.0, 180.0 / 255.0, 100.0 / 255.0, 1.0],
];

/// CPU 侧 fallback 轨道色（track_enc=0 视为第 0 轨）。
#[must_use]
pub fn fallback_track_color(track_enc: u16) -> [f32; 4] {
    let idx = track_enc.saturating_sub(1) as usize;
    FALLBACK_TRACK_COLORS[idx % FALLBACK_TRACK_COLORS.len()]
}

/// CPU 侧 0xRRGGBBAA 精确轨道色（与 shader u32 调色板逐字节一致）。
#[must_use]
pub fn fallback_track_color_u32(track_enc: u16) -> u32 {
    let idx = track_enc.saturating_sub(1) as usize;
    let c = FALLBACK_TRACK_COLORS_U8[idx % FALLBACK_TRACK_COLORS_U8.len()];
    (u32::from(c[0]) << 24) | (u32::from(c[1]) << 16) | (u32::from(c[2]) << 8) | 0xFF
}

/// 将 `(key, color)` 编码为 wasabi 风格的 `key_color` u32
/// 低 8 位 = key，高 24 位 = RGB（每通道 8 位，无 alpha）
#[must_use]
pub fn pack_key_color(key: u8, color: [f32; 4]) -> u32 {
    let r = (color[0].clamp(0.0, 1.0) * 255.0) as u32;
    let g = (color[1].clamp(0.0, 1.0) * 255.0) as u32;
    let b = (color[2].clamp(0.0, 1.0) * 255.0) as u32;
    let rgb = (r << 16) | (g << 8) | b;
    (key as u32) | (rgb << 8)
}

/// 将 `key_color` u32 解码为 `(key, rgba)`
/// alpha 不存于 `key_color`，恒为 1.0（与 wasabi 一致）
#[must_use]
pub fn unpack_key_color(packed: u32) -> (u8, [f32; 4]) {
    let key = (packed & 0xFF) as u8;
    let rgb = packed >> 8;
    let r = ((rgb >> 16) & 0xFF) as f32 / 255.0;
    let g = ((rgb >> 8) & 0xFF) as f32 / 255.0;
    let b = (rgb & 0xFF) as f32 / 255.0;
    (key, [r, g, b, 1.0])
}

impl NoteInstance {
    /// 创建新的音符逻辑实例（普通音符）
    /// `key` 为 u8（0-255，支持 256 键，与 wasabi `NoteVertex::new` 一致）
    /// `border_width` 低 16 位由 `calculate_border_width` 算出，主音轨所有音符共享同一值；
    /// 高 16 位为轨道深度编码（洋葱皮由 `onion_border_width` 编码，主音轨为 0）
    /// 创建普通实例。`color` 参数仅为迁移期 API 兼容保留：颜色由轨道调色板决定。
    #[must_use]
    pub fn new(tick: f32, key: u8, length: f32, _color: [f32; 4], border_width: u32) -> Self {
        let preview = border_width == PREVIEW_BORDER_SENTINEL;
        #[allow(unused_mut)]
        let mut track_enc = if preview {
            0
        } else {
            (border_width >> 16) as u16
        };
        let border = if preview { 0 } else { border_width as u8 };
        // 单测/GPU 像素测试常用不同 `color` 区分两个实例；12B 不存 RGB 后，
        // 仅在测试构建里把颜色散列成轨道，保证参考渲染仍可区分，生产不受影响。
        #[cfg(test)]
        if !preview && track_enc == 0 {
            let h = u32::from_le_bytes([
                (_color[0].clamp(0.0, 1.0) * 255.0) as u8,
                (_color[1].clamp(0.0, 1.0) * 255.0) as u8,
                (_color[2].clamp(0.0, 1.0) * 255.0) as u8,
                0,
            ]);
            // 4093 为质数，避免 0xFF/0xFF00 这类通道落在同一 12 色槽位。
            track_enc = 1 + (h % 4093) as u16;
        }
        Self {
            start_length: [tick, length],
            attrs: Self::encode_attrs(key, track_enc, border, preview),
        }
    }

    /// 创建预览音符（preview bit，FS 走预览分支）。
    #[must_use]
    pub fn new_preview(tick: f32, key: u8, length: f32, _color: [f32; 4]) -> Self {
        Self {
            start_length: [tick, length],
            attrs: Self::encode_attrs(key, 0, 0, true),
        }
    }
}

/// 计算音符边框像素宽度（复刻 wasabi `utils::calculate_border_width`）
///
/// 参考：`wasabi/src/utils.rs:13-15`
/// ```ignore
/// pub fn calculate_border_width(width_pixels: f32, keys_len: f32) -> f32 {
///     ((width_pixels / keys_len) / 12.0).clamp(1.0, 5.0).round() * 2.0
/// }
/// ```
///
/// 主音轨所有音符共享同一 `zoom.y`，因此 CPU 端只算一次填所有音符（D2=C 决策）。
#[must_use]
pub fn calculate_border_width(width_pixels: f32, keys_len: f32) -> u32 {
    if keys_len <= 0.0 {
        return 0;
    }
    let raw = ((width_pixels / keys_len) / 12.0).clamp(1.0, 5.0).round() * 2.0;
    raw as u32
}

/// 摄像机/视口 uniform 数据
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    /// 滚动偏移 [scroll_x, scroll_y]
    pub scroll: [f32; 2],
    /// 缩放 [zoom_x, zoom_y]
    pub zoom: [f32; 2],
    /// 视口尺寸 [width, height]
    pub viewport_size: [f32; 2],
    /// 画布偏移 [x, y]
    pub canvas_offset: [f32; 2],
    /// 画布尺寸 [width, height]（纵向头部对齐键盘顶部需用）
    pub canvas_size: [f32; 2],
    /// 键盘分区宽度（像素）
    pub keyboard_width: f32,
    /// 顶标尺高度（像素）
    pub ruler_height: f32,
    /// 最大琴键索引
    pub max_key_index: f32,
    /// 对齐填充（保持 16 字节对齐，64 字节）
    pub _padding: [f32; 3],
}

/// 相机/视口参数（摄像头 uniform 的友好输入类型）
pub struct CameraParams {
    /// 滚动偏移 [scroll_x, scroll_y]
    pub scroll: [f32; 2],
    /// 缩放 [zoom_x, zoom_y]
    pub zoom: [f32; 2],
    /// 视口尺寸 [width, height]
    pub viewport: [f32; 2],
    /// 画布偏移 [x, y]
    pub offset: [f32; 2],
    /// 画布尺寸 [width, height]（纵向头部对齐键盘顶部）
    pub canvas_size: [f32; 2],
    /// 键盘分区宽度（像素）
    pub keyboard_width: f32,
    /// 顶标尺高度（像素）
    pub ruler_height: f32,
    /// 最大琴键索引
    pub max_key_index: f32,
}

impl CameraUniform {
    /// 从 [`CameraParams`] 构建摄像头 uniform。
    ///
    /// # 参数
    /// * `params` — 相机/视口参数集合
    #[must_use]
    pub const fn new(params: CameraParams) -> Self {
        Self {
            scroll: params.scroll,
            zoom: params.zoom,
            viewport_size: params.viewport,
            canvas_offset: params.offset,
            canvas_size: params.canvas_size,
            keyboard_width: params.keyboard_width,
            ruler_height: params.ruler_height,
            max_key_index: params.max_key_index,
            _padding: [0.0; 3],
        }
    }
}

impl Default for CameraUniform {
    fn default() -> Self {
        Self {
            scroll: [0.0, 0.0],
            zoom: [1.0, 1.0],
            viewport_size: [0.0, 0.0],
            canvas_offset: [0.0, 0.0],
            canvas_size: [0.0, 0.0],
            keyboard_width: 0.0,
            ruler_height: 0.0,
            max_key_index: 0.0,
            _padding: [0.0; 3],
        }
    }
}

/// 裁剪 uniform 数据（每 chunk / 每裁剪区间一份，16 bytes）
///
/// storage binding 有 `max_storage_buffer_binding_size`（2GB-1）硬限制，
/// 超大数据按 chunk 切片绑定；`chunk_start`/`chunk_count` 描述本次分发
/// 在全局实例数组中的范围，`instance_count` 为全局实例数（冗余诊断），
/// `local_start` 为本次分发在 **chunk 局部** 的起始偏移（整段 cull 为 0，
/// 可见区间裁剪时为 `range.start - chunk_start`）。
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CullUniform {
    /// 全局实例总数（冗余诊断用）
    pub instance_count: u32,
    /// 本 chunk 在全局实例数组中的起始索引
    pub chunk_start: u32,
    /// 本次分发包含的实例数量
    pub chunk_count: u32,
    /// 本次分发在 chunk 局部的起始偏移（整段 cull = 0）
    pub local_start: u32,
}

/// 一次 GPU cull 的候选实例区间（全局实例索引，左闭右开）。
///
/// 由 UI 按相机可见 tick 窗口 + 每轨 max note length 算出，渲染线程据此
/// **只扫可见区间**，避免每帧全量读 N 个实例（实测 64M 音符全量 cull 3.53ms，
/// 其中绝大多数实例根本不在视口内）。
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub struct CullRange {
    /// 全局起始实例索引
    pub start: u32,
    /// 实例数量（> 0）
    pub count: u32,
}

/// 一帧的 cull 计划：候选区间 + 文档实例总数（一致性校验）。
///
/// `total_instances` 必须等于渲染线程当前 GPU 实例数；不等说明 UI 与渲染侧
/// 数据尚未同步（流式上传/增量在途），渲染线程回退整段 cull 保证正确性。
#[derive(Clone, Debug, Default)]
pub struct CullPlan {
    /// 候选区间（按 start 升序、互不重叠；空 = 整段 cull）
    pub ranges: Vec<CullRange>,
    /// 文档实例总数（一致性校验）
    pub total_instances: usize,
}

impl CullPlan {
    /// 计划是否可用（非空且总数已知）。
    #[must_use]
    pub fn is_active(&self) -> bool {
        !self.ranges.is_empty() && self.total_instances > 0
    }
}

/// 合并的渲染 uniform 数据（Camera + Cull）
/// 用于单次上传，减少 CPU-GPU 往返
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RenderUniform {
    /// 相机/视口 uniform
    pub camera: CameraUniform,
    /// 裁剪 uniform（chunk 范围描述）
    pub cull: CullUniform,
}

/// 视图状态 uniform：当前音轨 + 静音位图
///
/// 统一全量渲染（2026-08-06）：洋葱皮 buffer 持有所有轨全部音符，
/// 「哪个轨是主音轨 / 哪些轨静音」由本 uniform 低频更新（切轨/静音变化），
/// GPU 数据零重传——shader 据 `current_track` 把主音轨染成主轨色、
/// 深度压到主音轨区（`MAIN_DEPTH_REGION_BITS`，覆盖全部洋葱皮轨道）、
/// 静音轨仅主轨身份时显示。
///
/// 布局：`current_track`（track_idx+1，0=无）+ 2048 u32 静音位图（65536 轨上限，
/// 支持未来大编制工程）。字节布局与 shader `ViewState`（vec4 数组）严格一致：
/// 2048 u32 == 512 × vec4<`u32`>。
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ViewState {
    /// 当前音轨编码：track_idx + 1（0 = 无主音轨）
    pub current_track: u32,
    /// 对齐填充（保持 muted_bits 16 字节对齐）
    pub _padding: [u32; 3],
    /// 静音音轨位图：`muted_bits[t / 32]` 的 bit `t % 32` = 音轨 t 静音
    pub muted_bits: [u32; ViewState::MUTED_WORDS],
}

impl ViewState {
    /// 静音位图字数量（65536 轨上限，为超大编制工程预留）
    pub const MUTED_WORDS: usize = 2048;

    /// 空视图状态（无主音轨、无静音）
    #[must_use]
    pub fn new() -> Self {
        Self {
            current_track: 0,
            _padding: [0; 3],
            muted_bits: [0; Self::MUTED_WORDS],
        }
    }

    /// 设置音轨静音状态（越界防御：超出位图容量静默忽略）
    pub fn set_muted(&mut self, track: usize, muted: bool) {
        let word = track / 32;
        let bit = track % 32;
        if let Some(w) = self.muted_bits.get_mut(word) {
            if muted {
                *w |= 1 << bit;
            } else {
                *w &= !(1 << bit);
            }
        }
    }

    /// 查询音轨静音状态（越界返回 false）
    #[must_use]
    pub fn is_muted(&self, track: usize) -> bool {
        self.muted_bits
            .get(track / 32)
            .is_some_and(|w| (w >> (track % 32)) & 1 != 0)
    }
}

impl Default for ViewState {
    fn default() -> Self {
        Self::new()
    }
}

/// 间接绘制参数
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DrawIndirectArgs {
    pub vertex_count: u32,
    pub instance_count: u32,
    pub first_vertex: u32,
    pub first_instance: u32,
    pub _padding: [u32; 4],
}

impl Default for DrawIndirectArgs {
    fn default() -> Self {
        Self {
            vertex_count: 4,
            instance_count: 0,
            first_vertex: 0,
            first_instance: 0,
            _padding: [0; 4],
        }
    }
}

/// 可见索引顶点属性布局 — 4 bytes u32 源实例索引
/// render pass 用该索引从 all_instances storage buffer 读取完整 NoteInstance。
pub const VISIBLE_INDEX_ATTRIBUTES: [wgpu::VertexAttribute; 1] = [wgpu::VertexAttribute {
    offset: 0,
    shader_location: 0,
    format: wgpu::VertexFormat::Uint32,
}];
