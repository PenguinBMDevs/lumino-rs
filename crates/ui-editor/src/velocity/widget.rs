//! 力度/Tempo Canvas 程序
//!
//! 包含 Canvas Program trait 实现和事件处理逻辑。

mod drawing;
mod sections;
mod state;

pub use drawing::{
    bend_value_to_y, draw_background, draw_curve_paint_feedback, draw_horizontal_lines,
    draw_resize_handle, draw_scale_labels, draw_tempo_graph, draw_vertical_lines,
    generate_tempo_levels, tempo_bpm_to_y, tempo_point_screen_pos, velocity_bg_color,
    velocity_border_color, velocity_grab_bar_color, velocity_grid_line_color,
    velocity_handle_bg_color, velocity_text_color,
};
pub use state::VelocityCanvasState;

/// 速度控制点
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoPoint {
    /// tick 位置
    pub tick: f32,
    /// BPM 值 (20-10000)
    pub bpm: f64,
}

use super::EditMode;

/// 力度/Tempo Canvas 程序
pub struct VelocityCanvas<'a> {
    pub editor: &'a crate::Editor,
    /// 当前编辑模式
    pub edit_mode: EditMode,
}
