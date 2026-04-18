//! Camera Uniform 类型定义

use bytemuck::{Pod, Zeroable};

/// Camera Uniform
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct GridCameraUniform {
    pub viewport_size: [f32; 2],
    pub camera_pos: [f32; 2],
    pub zoom: [f32; 2],
    pub margins: [f32; 2],
    pub color_bg: [f32; 4],
    pub color_bg_black_key: [f32; 4],
    pub color_bar: [f32; 4],
    pub color_beat: [f32; 4],
    pub color_half_beat: [f32; 4],
    pub color_grid: [f32; 4],
    pub color_key_line: [f32; 4],
    pub ppq: f32,
    pub max_key_index: f32,
    pub canvas_offset: [f32; 2],
}

impl GridCameraUniform {
    pub fn new(
        viewport_width: f32,
        viewport_height: f32,
        scroll_x: f32,
        scroll_y: f32,
        zoom_x: f32,
        zoom_y: f32,
        keyboard_width: f32,
        ruler_height: f32,
        color_bg: [f32; 4],
        color_bg_black_key: [f32; 4],
        color_bar: [f32; 4],
        color_beat: [f32; 4],
        color_half_beat: [f32; 4],
        color_grid: [f32; 4],
        color_key_line: [f32; 4],
        ppq: f32,
        max_key_index: f32,
        canvas_offset_x: f32,
        canvas_offset_y: f32,
    ) -> Self {
        Self {
            viewport_size: [viewport_width, viewport_height],
            camera_pos: [scroll_x, scroll_y],
            zoom: [zoom_x, zoom_y],
            margins: [keyboard_width, ruler_height],
            color_bg,
            color_bg_black_key,
            color_bar,
            color_beat,
            color_half_beat,
            color_grid,
            color_key_line,
            ppq,
            max_key_index,
            canvas_offset: [canvas_offset_x, canvas_offset_y],
        }
    }
}
