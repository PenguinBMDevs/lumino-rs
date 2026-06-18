use std::sync::Arc;

use crate::{
    ArrangementNoteInstance, ArrangementUniform, CcBarInstance, GridLineInstance, KeyInstance,
    NoteInstance, OnionSkinBucket, RulerTickInstance,
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
    /// 洋葱皮轨道颜色表（per-track RGBA8 打包颜色，index = track_idx）
    pub onion_track_colors: Option<Vec<u32>>,
    /// 洋葱皮按 key 分桶缓存（渲染线程直接采集用）
    pub onion_bucket: Option<Arc<OnionSkinBucket>>,
    /// bucket 版本号，用于渲染线程检测数据变化
    pub onion_bucket_version: u64,
    /// 右侧 overscan ticks（补偿 fire-and-forget 模式下 buffer 滞后）
    pub onion_overscan_ticks: f32,
    /// 当前编辑音轨索引（采集时排除）
    pub onion_current_track: u16,
    /// 洋葱皮是否启用
    pub onion_enabled: bool,
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
            onion_track_colors: None,
            onion_bucket: None,
            onion_bucket_version: 0,
            onion_overscan_ticks: 0.0,
            onion_current_track: 0,
            onion_enabled: false,
        }
    }
}

impl RenderParams {
    /// 从收集的渲染数据和配置构建 RenderParams。
    ///
    /// 替代手动逐字段赋值，提供单一构造入口。
    #[allow(clippy::too_many_arguments)]
    pub fn from_data(
        // Physical/logical viewport
        physical_size: (u32, u32),
        logical_size: (f32, f32),
        scale_factor: f32,
        // Scroll/zoom
        scroll: (f32, f32),
        zoom: (f32, f32),
        // Canvas geometry
        keyboard_width: f32,
        ruler_height: f32,
        canvas_offset: (f32, f32),
        canvas_size: (f32, f32),
        // Colors
        background_color: [f64; 4],
        color_bg: [f32; 4],
        color_bg_black_key: [f32; 4],
        color_bar: [f32; 4],
        color_beat: [f32; 4],
        color_half_beat: [f32; 4],
        color_grid: [f32; 4],
        color_key_line: [f32; 4],
        // Timing
        ppq: f32,
        max_key_index: f32,
        // Mode
        is_arrangement_mode: bool,
        // Instances
        grid_instances: Vec<GridLineInstance>,
        ruler_instances: Vec<RulerTickInstance>,
        keyboard_instances: Vec<KeyInstance>,
        arrangement_note_instances: Vec<ArrangementNoteInstance>,
        arrangement_uniform: ArrangementUniform,
        cc_bar_instances: Vec<CcBarInstance>,
        velocity_panel_rect: Option<(f32, f32, f32, f32)>,
        // Onion skin (per-track packed colors)
        onion_track_colors: Option<Vec<u32>>,
        // Onion skin (render-thread collection)
        onion_bucket: Option<Arc<OnionSkinBucket>>,
        onion_bucket_version: u64,
        onion_overscan_ticks: f32,
        onion_current_track: u16,
        onion_enabled: bool,
    ) -> Self {
        Self {
            viewport_size: (physical_size.0, physical_size.1),
            logical_size,
            scale_factor,
            scroll,
            zoom,
            keyboard_width,
            ruler_height,
            background_color,
            color_bg,
            color_bg_black_key,
            color_bar,
            color_beat,
            color_half_beat,
            color_grid,
            color_key_line,
            grid_instances,
            note_instances: Vec::new(),
            ruler_instances,
            keyboard_instances,
            ticks_per_measure: (ppq as u32) * 4,
            ticks_per_beat: ppq as u32,
            regenerate_grid: false,
            canvas_offset,
            canvas_size,
            ppq,
            max_key_index,
            is_arrangement_mode,
            arrangement_note_instances,
            arrangement_uniform,
            cc_bar_instances,
            velocity_panel_rect,
            onion_track_colors,
            onion_bucket,
            onion_bucket_version,
            onion_overscan_ticks,
            onion_current_track,
            onion_enabled,
        }
    }
}
