use crate::{
    ArrangementNoteInstance, ArrangementUniform, CcBarInstance, GridLineInstance,
    MiditrailNoteGpu, NoteInstance, RulerTickInstance, WaterfallNoteGpu,
};

mod builder;

pub use builder::RenderParamsBuilder;

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
    /// 黑键区域背景色 (RGBA)
    pub color_bg_black_key: [f32; 4],
    /// 小节线颜色 (RGBA)
    pub color_bar: [f32; 4],
    /// 拍子线颜色 (RGBA)
    pub color_beat: [f32; 4],
    /// 半拍线颜色 (RGBA)
    pub color_half_beat: [f32; 4],
    /// 细分网格线颜色 (RGBA)
    pub color_grid: [f32; 4],
    /// 琴键分隔线颜色 (RGBA)
    pub color_key_line: [f32; 4],
    /// 网格线实例
    pub grid_instances: Vec<GridLineInstance>,
    /// 音符实例
    pub note_instances: Vec<NoteInstance>,
    /// 标尺刻度实例
    pub ruler_instances: Vec<RulerTickInstance>,
    /// 每小节 tick 数
    pub ticks_per_measure: u32,
    /// 每拍 tick 数
    pub ticks_per_beat: u32,
    /// 拍号变化列表 (tick, 分子, 分母)
    pub time_signatures: Vec<(u32, u8, u8)>,
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
    /// 是否为瀑布流渲染模式
    pub is_waterfall_mode: bool,
    /// 瀑布流滚动速度
    pub waterfall_speed: f32,
    /// 瀑布流 GPU 音符数据（仅在瀑布流模式下使用）
    pub waterfall_notes: Vec<WaterfallNoteGpu>,
    /// 瀑布流音符按 key 分桶的偏移表（129 个 u32：key i 的起始索引为
    /// `waterfall_key_offsets[i]`，结束为 `waterfall_key_offsets[i+1]`）。
    /// `waterfall_notes` 按 `(key, start_tick)` 升序排列，shader 借此实现
    /// 每像素 O(1) 定位 + 二分回溯，避免 10W+ 密集音符时全量遍历。
    pub waterfall_key_offsets: Vec<u32>,
    /// 瀑布流当前 MIDI tick 值（与 scroll.0 不同，scroll.0 是像素位置）
    pub waterfall_current_tick: u32,
    /// Miditrail 渲染开关（非 None 时走 Miditrail 3D GPU 渲染器）
    pub miditrail_enabled: bool,
    /// Miditrail 滚动速度
    pub miditrail_speed: f32,
    /// Miditrail GPU 音符数据
    pub miditrail_notes: Vec<MiditrailNoteGpu>,
    /// Miditrail 当前 MIDI tick 值
    pub miditrail_current_tick: u32,
    /// Miditrail Z 方向显示距离（音符在多远被截断）。
    pub miditrail_z_far: f32,
    /// Miditrail 光晕环动画时间基准：当前 tick 处每秒 tick 数（BPM × ppq / 60）。
    ///
    /// 0 表示未知（由渲染线程回退到 120 BPM 估算），供非导出路径的默认参数使用。
    pub miditrail_ticks_per_second: f32,
    /// Miditrail / 视频导出目标帧率（用于按键动画时间步长）。
    pub fps: f32,
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
            ticks_per_measure: 7680,
            ticks_per_beat: 1920,
            canvas_offset: (0.0, 0.0),
            canvas_size: (800.0, 600.0),
            ppq: 1920.0,
            max_key_index: 127.0,
            is_arrangement_mode: false,
            arrangement_note_instances: Vec::new(),
            arrangement_uniform: ArrangementUniform::default(),
            cc_bar_instances: Vec::new(),
            velocity_panel_rect: None,
            time_signatures: vec![(0, 4, 4)],
            is_waterfall_mode: false,
            waterfall_speed: 1.0,
            waterfall_notes: Vec::new(),
            waterfall_key_offsets: Vec::new(),
            waterfall_current_tick: 0,
            miditrail_enabled: false,
            miditrail_speed: 1.0,
            miditrail_notes: Vec::new(),
            miditrail_current_tick: 0,
            miditrail_z_far: 7.5,
            miditrail_ticks_per_second: 0.0,
            fps: 60.0,
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
