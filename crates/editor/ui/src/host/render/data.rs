use iced_core::Size;

/// 渲染数据
pub struct RenderData {
    pub scroll: (f32, f32),
    pub zoom: (f32, f32),
    pub viewport_size: Size,
    pub ruler_instances: Vec<lumino_gfx::RulerTickInstance>,
    pub arrangement_note_instances: Vec<lumino_gfx::ArrangementNoteInstance>,
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
