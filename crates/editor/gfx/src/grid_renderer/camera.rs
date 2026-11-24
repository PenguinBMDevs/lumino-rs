//! 网格渲染器 — 相机 Uniform 默认值与 prepare 参数转换

use super::*;

impl Default for GridCameraUniform {
    fn default() -> Self {
        // 与旧 Builder 默认值逐字段一致；时间签名默认一条 4/4。
        let mut time_signatures = [[0u32; 4]; 16];
        time_signatures[0] = [0, 4, 4, 0];
        Self {
            viewport_size: [1.0, 1.0],
            camera_pos: [0.0, 0.0],
            zoom: [1.0, 1.0],
            margins: [0.0, 0.0],
            color_bg: [0.1, 0.1, 0.1, 1.0],
            color_bg_black_key: [0.07, 0.07, 0.07, 1.0],
            color_bar: [0.3, 0.3, 0.3, 1.0],
            color_beat: [0.2, 0.2, 0.2, 1.0],
            color_half_beat: [0.15, 0.15, 0.15, 1.0],
            color_grid: [0.15, 0.15, 0.15, 1.0],
            color_key_line: [0.15, 0.15, 0.15, 1.0],
            ppq: 1920.0,
            max_key_index: 127.0,
            canvas_offset: [0.0, 0.0],
            canvas_size: [800.0, 600.0],
            time_signature_count: 1,
            _padding: [0; 1],
            time_signatures,
        }
    }
}

impl GridCameraUniform {
    /// 从 prepare 参数构造（时间签名截断到 16 条）。
    pub(crate) fn from_prepare_params(params: &GridPrepareParams) -> Self {
        let mut time_signatures = [[0u32; 4]; 16];
        let count = params.time_signatures.len().min(16) as u32;
        for (i, (tick, num, den)) in params.time_signatures.iter().take(16).enumerate() {
            time_signatures[i] = [*tick, *num as u32, *den as u32, 0];
        }
        Self {
            viewport_size: [params.viewport_size.0, params.viewport_size.1],
            camera_pos: [params.scroll_x, params.scroll_y],
            zoom: [params.zoom_x, params.zoom_y],
            margins: [params.keyboard_width, params.ruler_height],
            color_bg: params.color_bg,
            color_bg_black_key: params.color_bg_black_key,
            color_bar: params.color_bar,
            color_beat: params.color_beat,
            color_half_beat: params.color_half_beat,
            color_grid: params.color_grid,
            color_key_line: params.color_key_line,
            ppq: params.ppq,
            max_key_index: params.max_key_index,
            canvas_offset: [params.canvas_offset_x, params.canvas_offset_y],
            canvas_size: [params.canvas_size.0, params.canvas_size.1],
            time_signature_count: count,
            _padding: [0; 1],
            time_signatures,
        }
    }
}
