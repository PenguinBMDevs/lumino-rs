use iced_core::Size;

/// 渲染数据
pub struct RenderData {
    pub scroll: (f32, f32),
    pub zoom: (f32, f32),
    pub viewport_size: Size,
    pub ruler_instances: Vec<lumino_gfx::RulerTickInstance>,
    /// 走带视图覆盖层实例（背景/lane/网格/框选/指示线），每帧重建
    pub arrangement_overlay_instances: Vec<lumino_gfx::ArrangementNoteInstance>,
    /// 覆盖层中"背景层"实例数（背景/lane/网格），绘制在音符之下
    pub arrangement_overlay_back_len: usize,
    /// 走带视图侧栏音轨顺序（文档音轨 id 列表，索引=泳道序号）。
    ///
    /// 走带音符直接复用钢琴卷帘常驻 GPU 音符缓冲（零第二份显存），
    /// 该顺序用于把文档音轨映射到泳道序号，由渲染线程段表定位可见音符。
    pub arrangement_track_order: Vec<usize>,
    /// 走带视图各泳道可见性（与 `arrangement_track_order` 对齐，`false`=静音/隐藏不绘制）。
    pub arrangement_track_visible: Vec<bool>,
    pub cc_bar_instances: Vec<lumino_gfx::CcBarInstance>,
}

/// 视口信息
pub struct ViewportInfo {
    pub canvas_offset: iced_core::Point,
    pub canvas_size: iced_core::Point,
}

/// 从主题提取的网格渲染颜色
pub struct GridColors {
    pub bg: [f32; 4],
    pub black_key: [f32; 4],
    pub bar_line: [f32; 4],
    pub beat_line: [f32; 4],
    pub half_beat_line: [f32; 4],
    pub grid_line: [f32; 4],
    pub key_line: [f32; 4],
}

impl GridColors {
    pub fn from_theme(theme: &crate::Theme) -> Self {
        use crate::editor::grid::theme::ThemeExt;
        let c_bg = theme.keyboard_background_color();
        let c_bk = theme.black_key_color();
        let c_bar = theme.bar_line_color();
        let c_beat = theme.beat_line_color();
        let c_half = theme.half_beat_line_color();
        let c_grid = theme.grid_line_color();
        let palette = theme.extended_palette().background;
        let c_kl = if theme.is_light() {
            palette.strong.color
        } else {
            palette.weak.color
        };

        Self {
            bg: [c_bg.r, c_bg.g, c_bg.b, c_bg.a],
            black_key: [c_bk.r, c_bk.g, c_bk.b, c_bk.a],
            bar_line: [c_bar.r, c_bar.g, c_bar.b, c_bar.a],
            beat_line: [c_beat.r, c_beat.g, c_beat.b, c_beat.a],
            half_beat_line: [c_half.r, c_half.g, c_half.b, c_half.a],
            grid_line: [c_grid.r, c_grid.g, c_grid.b, c_grid.a],
            key_line: [c_kl.r, c_kl.g, c_kl.b, c_kl.a],
        }
    }
}
