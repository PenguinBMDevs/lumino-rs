//! 工程走带渲染器类型定义
//! 参考 yinhe 实现，使用实例化渲染 (Instance Rendering)

/// 走带视图 Uniform 数据
/// 注意：WGSL 内存对齐规则严格，vec2 对齐到 8 字节，vec4 对齐到 16 字节
/// 总大小：368 字节
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ArrangementUniform {
    /// 视口尺寸 [width, height] — offset 0
    pub viewport_size: [f32; 2],
    /// 滚动偏移 [scroll_x, scroll_y] — offset 8
    pub scroll: [f32; 2],
    /// 缩放 (pixels_per_tick) — offset 16
    pub zoom: f32,
    /// 每轨高度（像素）— offset 20
    pub track_height: f32,
    /// 每轨音符数（128）— offset 24
    pub notes_per_track: f32,
    /// 对齐填充 — offset 28 (align canvas_offset to 8)
    pub _pad0: f32,
    /// 画布偏移 [x, y] — offset 32
    pub canvas_offset: [f32; 2],
    /// 演奏指示线 x 坐标 — offset 40
    pub playhead_x: f32,
    /// 对齐填充 — offset 44 (align bg_color to 16)
    pub _pad1: f32,
    /// 背景色 — offset 48
    pub bg_color: [f32; 4],
    /// 小节线颜色 — offset 64
    pub bar_color: [f32; 4],
    /// 演奏指示线颜色 — offset 80
    pub playhead_color: [f32; 4],
    /// 音轨颜色调色板 (16 个 vec4，每个 stride=16) — offset 96
    pub track_colors: [[f32; 4]; 16],
    /// 音轨数量 — offset 352
    pub track_count: f32,
    /// 对齐填充 — offset 356
    pub _pad2: f32,
    /// 对齐填充 — offset 360
    pub _pad3: f32,
    /// 对齐填充 — offset 364
    pub _pad4: f32,
}

impl Default for ArrangementUniform {
    fn default() -> Self {
        Self {
            viewport_size: [800.0, 600.0],
            scroll: [0.0, 0.0],
            zoom: 0.5,
            track_height: 48.0,
            notes_per_track: 128.0,
            canvas_offset: [0.0, 0.0],
            playhead_x: -1.0,
            _pad0: 0.0,
            _pad1: 0.0,
            bg_color: [0.18, 0.18, 0.18, 1.0],
            bar_color: [0.3, 0.3, 0.3, 1.0],
            playhead_color: [1.0, 0.2, 0.2, 1.0],
            track_colors: [[0.0; 4]; 16],
            track_count: 0.0,
            _pad2: 0.0,
            _pad3: 0.0,
            _pad4: 0.0,
        }
    }
}

/// 走带视图音符实例数据 — 32字节
/// 与 yinhe 的 NoteInstance 兼容
/// Layout: xywh (vec4 f32) + packed (vec4 u32) = 2 vertex attributes
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ArrangementNoteInstance {
    /// 屏幕空间坐标: x (横坐标，像素)
    pub x: f32,
    /// 屏幕空间坐标: y (纵坐标，像素)
    pub y: f32,
    /// 实例宽度 (像素)
    pub w: f32,
    /// 实例高度 (像素)
    pub h: f32,
    /// RGBA 打包为 4x UNORM8: R|G<<8|B<<16|A<<24
    pub rgba_packed: u32,
    /// corner_radius (f16 high) | border_width (f16 low)
    pub props_packed: u32,
    /// 保留/velocity
    pub velocity: u32,
    /// 语义标签: 0=背景, 1=lane, 2=网格线, 3=音符, 4=演奏指示线
    pub tag: u32,
}

impl ArrangementNoteInstance {
    /// 创建背景实例
    pub fn background(x: f32, y: f32, w: f32, h: f32, color: [f32; 3]) -> Self {
        Self {
            x,
            y,
            w,
            h,
            rgba_packed: pack_rgba(color[0], color[1], color[2], 1.0),
            props_packed: pack_props(0.0, 0.0),
            velocity: 0,
            tag: 0,
        }
    }

    /// 创建音轨 lane 背景实例
    pub fn lane(x: f32, y: f32, w: f32, h: f32, color: [f32; 3]) -> Self {
        Self {
            x,
            y,
            w,
            h,
            rgba_packed: pack_rgba(color[0], color[1], color[2], 1.0),
            props_packed: pack_props(0.0, 0.0),
            velocity: 0,
            tag: 1,
        }
    }

    /// 创建网格线实例
    pub fn grid_line(x: f32, y: f32, w: f32, h: f32, color: [f32; 4], tick: u32) -> Self {
        Self {
            x,
            y,
            w,
            h,
            rgba_packed: pack_rgba(color[0], color[1], color[2], color[3]),
            props_packed: pack_props(0.0, 0.0),
            velocity: tick,
            tag: 2,
        }
    }

    /// 创建音符实例（屏幕坐标）
    pub fn note(x: f32, y: f32, w: f32, h: f32, color: [f32; 3], velocity: u8) -> Self {
        Self {
            x,
            y,
            w: w.max(2.0), // 最小宽度 2 像素
            h,
            rgba_packed: pack_rgba(color[0], color[1], color[2], 0.85),
            props_packed: pack_props(2.0, 1.0), // 圆角 2px, 边框 1px
            velocity: velocity as u32,
            tag: 3,
        }
    }

    /// 创建演奏指示线实例
    pub fn playhead(x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> Self {
        Self {
            x,
            y,
            w,
            h,
            rgba_packed: pack_rgba(color[0], color[1], color[2], color[3]),
            props_packed: pack_props(0.0, 0.0),
            velocity: 0,
            tag: 4,
        }
    }

    /// 创建 ghost 音符预览实例
    pub fn ghost_note(x: f32, y: f32, w: f32, h: f32, color: [f32; 3]) -> Self {
        Self {
            x,
            y,
            w: w.max(2.0),
            h,
            rgba_packed: pack_rgba(color[0], color[1], color[2], 0.5),
            props_packed: pack_props(2.0, 1.0),
            velocity: 0,
            tag: 5,
        }
    }

    /// 创建框选矩形实例（屏幕坐标，带半透明填充和描边）
    ///
    /// 与钢琴卷帘框选框统一样式：灰色填充（alpha 0.35）+ 3px 边框，
    /// 边框色由 shader 按 `BORDER_DARKEN_FACTOR` 自动加深（比填充深）。
    pub fn selection_rect(x: f32, y: f32, w: f32, h: f32, color: [f32; 3]) -> Self {
        Self {
            x,
            y,
            w: w.max(1.0),
            h: h.max(1.0),
            rgba_packed: pack_rgba(color[0], color[1], color[2], 0.35),
            props_packed: pack_props(0.0, 3.0),
            velocity: 0,
            tag: 6,
        }
    }
}

/// Pack RGBA floats (0.0-1.0) into a single u32 (UNORM8 x 4)
pub fn pack_rgba(r: f32, g: f32, b: f32, a: f32) -> u32 {
    let r8 = (r.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
    let g8 = (g.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
    let b8 = (b.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
    let a8 = (a.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
    r8 | (g8 << 8) | (b8 << 16) | (a8 << 24)
}

/// Pack corner_radius and border_width (both f32) into a single u32 (2x f16)
pub fn pack_props(corner_radius: f32, border_width: f32) -> u32 {
    let cr = half::f16::from_f32(corner_radius);
    let bw = half::f16::from_f32(border_width);
    (cr.to_bits() as u32) | ((bw.to_bits() as u32) << 16)
}

/// 走带视图网格颜色配置 (参考 yinhe)
pub mod colors {
    /// 背景色
    pub const AR_BG_COLOR: (f32, f32, f32) = (0.14, 0.14, 0.16);
    /// 偶数轨 lane 背景
    pub const AR_LANE_EVEN_COLOR: (f32, f32, f32) = (0.16, 0.16, 0.18);
    /// 奇数轨 lane 背景
    pub const AR_LANE_ODD_COLOR: (f32, f32, f32) = (0.13, 0.13, 0.15);
    /// 小节线颜色
    pub const AR_MEASURE_LINE_COLOR: (f32, f32, f32, f32) = (0.30, 0.30, 0.35, 1.0);
    /// 拍子线颜色
    pub const AR_BEAT_LINE_COLOR: (f32, f32, f32, f32) = (0.20, 0.20, 0.23, 1.0);
    /// 演奏指示线颜色（与钢琴卷帘统一为红色）
    pub const AR_PLAYHEAD_COLOR: (f32, f32, f32, f32) = (1.0, 0.2, 0.2, 1.0);
}
