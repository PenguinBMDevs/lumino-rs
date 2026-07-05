use crate::{
    ArrangementNoteInstance, ArrangementUniform, CcBarInstance, GridLineInstance, KeyInstance,
    NoteInstance, RulerTickInstance,
};

/// 渲染参数 - 从 UI 线程传递到 WGPU 线程
#[derive(Debug, Clone)]
pub struct RenderParams {
    /// 物理视口大小
    pub viewport_size: (u32, u32),
    /// 逻辑视口大小
    pub logical_size: (f32, f32),
    /// 缩放因子
    pub scale_factor: f32,
    /// 滚动位置 (x, y)
    pub scroll: (f32, f32),
    /// 缩放 (x, y)
    pub zoom: (f32, f32),
    /// 键盘宽度
    pub keyboard_width: f32,
    /// 标尺高度
    pub ruler_height: f32,
    /// 背景颜色
    pub background_color: [f64; 4],
    /// 网格相关颜色 (用于 Shader)
    pub color_bg: [f32; 4],
    pub color_bg_black_key: [f32; 4],
    pub color_bar: [f32; 4],
    pub color_beat: [f32; 4],
    pub color_half_beat: [f32; 4],
    pub color_grid: [f32; 4],
    pub color_key_line: [f32; 4],
    /// 网格线实例
    pub grid_instances: Vec<GridLineInstance>,
    /// 音符实例
    pub note_instances: Vec<NoteInstance>,
    /// 标尺刻度实例
    pub ruler_instances: Vec<RulerTickInstance>,
    /// 琴键实例
    pub keyboard_instances: Vec<KeyInstance>,
    /// 每小节 tick 数
    pub ticks_per_measure: u32,
    /// 每拍 tick 数
    pub ticks_per_beat: u32,
    /// 是否需要重新生成网格
    pub regenerate_grid: bool,
    /// Canvas 偏移
    pub canvas_offset: (f32, f32),
    /// Canvas 大小
    pub canvas_size: (f32, f32),
    /// 分辨率 (Pulses Per Quarter note)
    pub ppq: f32,
    /// 最大键索引 (visible_key_count - 1)
    pub max_key_index: f32,
    /// 是否为音轨总览模式（音轨总览模式下不渲染钢琴卷帘网格）
    pub is_arrangement_mode: bool,
    /// 音轨总览模式：音符实例
    pub arrangement_note_instances: Vec<ArrangementNoteInstance>,
    /// 音轨总览模式：uniform
    pub arrangement_uniform: ArrangementUniform,
    /// CC 柱状条实例（力度面板所有模式：Velocity/CC/Bend）
    pub cc_bar_instances: Vec<CcBarInstance>,
    /// 力度面板区域 (x, y, width, height) — 屏幕坐标，用于 scissor
    pub velocity_panel_rect: Option<(f32, f32, f32, f32)>,
}

impl Default for RenderParams {
    fn default() -> Self {
        Self {
            viewport_size: (800, 600),
            logical_size: (800.0, 600.0),
            scale_factor: 1.0,
            scroll: (0.0, 0.0),
            zoom: (0.1, 20.0),
            keyboard_width: 60.0,
            ruler_height: 30.0,
            background_color: [0.1, 0.1, 0.1, 1.0],
            color_bg: [0.1, 0.1, 0.1, 1.0],
            color_bg_black_key: [0.07, 0.07, 0.07, 1.0],
            color_bar: [0.3, 0.3, 0.3, 1.0],
            color_beat: [0.2, 0.2, 0.2, 1.0],
            color_half_beat: [0.15, 0.15, 0.15, 1.0],
            color_grid: [0.15, 0.15, 0.15, 1.0],
            color_key_line: [0.4, 0.4, 0.4, 1.0],
            grid_instances: Vec::new(),
            note_instances: Vec::new(),
            ruler_instances: Vec::new(),
            keyboard_instances: Vec::new(),
            ticks_per_measure: 7680,
            ticks_per_beat: 1920,
            regenerate_grid: true,
            canvas_offset: (0.0, 0.0),
            canvas_size: (800.0, 600.0),
            ppq: 1920.0,
            max_key_index: 127.0,
            is_arrangement_mode: false,
            arrangement_note_instances: Vec::new(),
            arrangement_uniform: ArrangementUniform::default(),
            cc_bar_instances: Vec::new(),
            velocity_panel_rect: None,
        }
    }
}

impl RenderParams {
    /// 创建 Builder，推荐的构造方式。
    ///
    /// ```ignore
    /// RenderParams::builder()
    ///     .viewport_size((1920, 1080))
    ///     .scroll((100.0, 50.0))
    ///     .zoom((0.5, 10.0))
    ///     .build()
    /// ```
    pub fn builder() -> RenderParamsBuilder {
        RenderParamsBuilder::default()
    }
}

/// [`RenderParams`] 的 Builder。
///
/// 所有字段均有合理默认值，只需设置需要变更的字段即可。
#[derive(Debug, Clone)]
pub struct RenderParamsBuilder {
    viewport_size: (u32, u32),
    logical_size: (f32, f32),
    scale_factor: f32,
    scroll: (f32, f32),
    zoom: (f32, f32),
    keyboard_width: f32,
    ruler_height: f32,
    background_color: [f64; 4],
    color_bg: [f32; 4],
    color_bg_black_key: [f32; 4],
    color_bar: [f32; 4],
    color_beat: [f32; 4],
    color_half_beat: [f32; 4],
    color_grid: [f32; 4],
    color_key_line: [f32; 4],
    grid_instances: Vec<GridLineInstance>,
    ruler_instances: Vec<RulerTickInstance>,
    keyboard_instances: Vec<KeyInstance>,
    ppq: f32,
    max_key_index: f32,
    is_arrangement_mode: bool,
    arrangement_note_instances: Vec<ArrangementNoteInstance>,
    arrangement_uniform: ArrangementUniform,
    cc_bar_instances: Vec<CcBarInstance>,
    canvas_offset: (f32, f32),
    canvas_size: (f32, f32),
    velocity_panel_rect: Option<(f32, f32, f32, f32)>,
}

impl Default for RenderParamsBuilder {
    fn default() -> Self {
        Self {
            viewport_size: (800, 600),
            logical_size: (800.0, 600.0),
            scale_factor: 1.0,
            scroll: (0.0, 0.0),
            zoom: (0.1, 20.0),
            keyboard_width: 60.0,
            ruler_height: 30.0,
            background_color: [0.1, 0.1, 0.1, 1.0],
            color_bg: [0.1, 0.1, 0.1, 1.0],
            color_bg_black_key: [0.07, 0.07, 0.07, 1.0],
            color_bar: [0.3, 0.3, 0.3, 1.0],
            color_beat: [0.2, 0.2, 0.2, 1.0],
            color_half_beat: [0.15, 0.15, 0.15, 1.0],
            color_grid: [0.15, 0.15, 0.15, 1.0],
            color_key_line: [0.4, 0.4, 0.4, 1.0],
            grid_instances: Vec::new(),
            ruler_instances: Vec::new(),
            keyboard_instances: Vec::new(),
            ppq: 1920.0,
            max_key_index: 127.0,
            is_arrangement_mode: false,
            arrangement_note_instances: Vec::new(),
            arrangement_uniform: ArrangementUniform::default(),
            cc_bar_instances: Vec::new(),
            canvas_offset: (0.0, 0.0),
            canvas_size: (800.0, 600.0),
            velocity_panel_rect: None,
        }
    }
}

impl RenderParamsBuilder {
    /// 设置物理视口大小
    pub fn viewport_size(mut self, size: (u32, u32)) -> Self {
        self.viewport_size = size;
        self
    }

    /// 设置逻辑视口大小
    pub fn logical_size(mut self, size: (f32, f32)) -> Self {
        self.logical_size = size;
        self
    }

    /// 设置缩放因子
    pub fn scale_factor(mut self, factor: f32) -> Self {
        self.scale_factor = factor;
        self
    }

    /// 设置滚动位置
    pub fn scroll(mut self, scroll: (f32, f32)) -> Self {
        self.scroll = scroll;
        self
    }

    /// 设置缩放
    pub fn zoom(mut self, zoom: (f32, f32)) -> Self {
        self.zoom = zoom;
        self
    }

    /// 设置键盘宽度
    pub fn keyboard_width(mut self, width: f32) -> Self {
        self.keyboard_width = width;
        self
    }

    /// 设置标尺高度
    pub fn ruler_height(mut self, height: f32) -> Self {
        self.ruler_height = height;
        self
    }

    /// 设置背景颜色
    pub fn background_color(mut self, color: [f64; 4]) -> Self {
        self.background_color = color;
        self
    }

    /// 设置网格背景色
    pub fn color_bg(mut self, color: [f32; 4]) -> Self {
        self.color_bg = color;
        self
    }

    /// 设置黑键背景色
    pub fn color_bg_black_key(mut self, color: [f32; 4]) -> Self {
        self.color_bg_black_key = color;
        self
    }

    /// 设置小节线颜色
    pub fn color_bar(mut self, color: [f32; 4]) -> Self {
        self.color_bar = color;
        self
    }

    /// 设置拍线颜色
    pub fn color_beat(mut self, color: [f32; 4]) -> Self {
        self.color_beat = color;
        self
    }

    /// 设置半拍线颜色
    pub fn color_half_beat(mut self, color: [f32; 4]) -> Self {
        self.color_half_beat = color;
        self
    }

    /// 设置网格线颜色
    pub fn color_grid(mut self, color: [f32; 4]) -> Self {
        self.color_grid = color;
        self
    }

    /// 设置键位线颜色
    pub fn color_key_line(mut self, color: [f32; 4]) -> Self {
        self.color_key_line = color;
        self
    }

    /// 设置网格线实例
    pub fn grid_instances(mut self, instances: Vec<GridLineInstance>) -> Self {
        self.grid_instances = instances;
        self
    }

    /// 设置标尺刻度实例
    pub fn ruler_instances(mut self, instances: Vec<RulerTickInstance>) -> Self {
        self.ruler_instances = instances;
        self
    }

    /// 设置琴键实例
    pub fn keyboard_instances(mut self, instances: Vec<KeyInstance>) -> Self {
        self.keyboard_instances = instances;
        self
    }

    /// 设置 PPQ (Pulses Per Quarter note)
    pub fn ppq(mut self, ppq: f32) -> Self {
        self.ppq = ppq;
        self
    }

    /// 设置最大键索引
    pub fn max_key_index(mut self, index: f32) -> Self {
        self.max_key_index = index;
        self
    }

    /// 设置是否为音轨总览模式
    pub fn is_arrangement_mode(mut self, mode: bool) -> Self {
        self.is_arrangement_mode = mode;
        self
    }

    /// 设置音轨总览模式音符实例
    pub fn arrangement_note_instances(mut self, instances: Vec<ArrangementNoteInstance>) -> Self {
        self.arrangement_note_instances = instances;
        self
    }

    /// 设置音轨总览模式 uniform
    pub fn arrangement_uniform(mut self, uniform: ArrangementUniform) -> Self {
        self.arrangement_uniform = uniform;
        self
    }

    /// 设置 CC 柱状条实例
    pub fn cc_bar_instances(mut self, instances: Vec<CcBarInstance>) -> Self {
        self.cc_bar_instances = instances;
        self
    }

    /// 设置 Canvas 偏移
    pub fn canvas_offset(mut self, offset: (f32, f32)) -> Self {
        self.canvas_offset = offset;
        self
    }

    /// 设置 Canvas 大小
    pub fn canvas_size(mut self, size: (f32, f32)) -> Self {
        self.canvas_size = size;
        self
    }

    /// 设置力度面板区域
    pub fn velocity_panel_rect(mut self, rect: Option<(f32, f32, f32, f32)>) -> Self {
        self.velocity_panel_rect = rect;
        self
    }

    /// 构建 [`RenderParams`]。
    ///
    /// 自动从 `ppq` 推导 `ticks_per_measure` 和 `ticks_per_beat`，
    /// `note_instances` 和 `regenerate_grid` 使用默认值。
    pub fn build(self) -> RenderParams {
        RenderParams {
            viewport_size: self.viewport_size,
            logical_size: self.logical_size,
            scale_factor: self.scale_factor,
            scroll: self.scroll,
            zoom: self.zoom,
            keyboard_width: self.keyboard_width,
            ruler_height: self.ruler_height,
            background_color: self.background_color,
            color_bg: self.color_bg,
            color_bg_black_key: self.color_bg_black_key,
            color_bar: self.color_bar,
            color_beat: self.color_beat,
            color_half_beat: self.color_half_beat,
            color_grid: self.color_grid,
            color_key_line: self.color_key_line,
            grid_instances: self.grid_instances,
            note_instances: Vec::new(),
            ruler_instances: self.ruler_instances,
            keyboard_instances: self.keyboard_instances,
            ticks_per_measure: (self.ppq as u32) * 4,
            ticks_per_beat: self.ppq as u32,
            regenerate_grid: false,
            canvas_offset: self.canvas_offset,
            canvas_size: self.canvas_size,
            ppq: self.ppq,
            max_key_index: self.max_key_index,
            is_arrangement_mode: self.is_arrangement_mode,
            arrangement_note_instances: self.arrangement_note_instances,
            arrangement_uniform: self.arrangement_uniform,
            cc_bar_instances: self.cc_bar_instances,
            velocity_panel_rect: self.velocity_panel_rect,
        }
    }
}
