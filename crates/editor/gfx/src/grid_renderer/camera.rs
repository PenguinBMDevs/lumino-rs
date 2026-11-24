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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_params(time_signatures: Vec<(u32, u8, u8)>) -> GridPrepareParams {
        GridPrepareParams {
            viewport_size: (1920.0, 1080.0),
            scroll_x: 100.0,
            scroll_y: 50.0,
            zoom_x: 0.5,
            zoom_y: 10.0,
            keyboard_width: 60.0,
            ruler_height: 30.0,
            color_bg: [0.11, 0.12, 0.13, 1.0],
            color_bg_black_key: [0.21, 0.22, 0.23, 1.0],
            color_bar: [0.31, 0.32, 0.33, 1.0],
            color_beat: [0.41, 0.42, 0.43, 1.0],
            color_half_beat: [0.51, 0.52, 0.53, 1.0],
            color_grid: [0.61, 0.62, 0.63, 1.0],
            color_key_line: [0.71, 0.72, 0.73, 1.0],
            ppq: 960.0,
            max_key_index: 127.0,
            canvas_offset_x: 10.0,
            canvas_offset_y: 20.0,
            canvas_size: (800.0, 600.0),
            time_signatures,
        }
    }

    /// 纵向网格准备参数逐字段映射到 uniform（X=key、Y=tick 的转置由 shader
    /// 完成，uniform 只承载原始数值）。
    #[test]
    fn from_prepare_params_maps_all_fields() {
        let params = sample_params(vec![(0, 4, 4), (3840, 3, 4)]);
        let u = GridCameraUniform::from_prepare_params(&params);

        assert_eq!(u.viewport_size, [1920.0, 1080.0]);
        assert_eq!(u.camera_pos, [100.0, 50.0]);
        assert_eq!(u.zoom, [0.5, 10.0]);
        assert_eq!(u.margins, [60.0, 30.0]);
        assert_eq!(u.color_bg, [0.11, 0.12, 0.13, 1.0]);
        assert_eq!(u.color_key_line, [0.71, 0.72, 0.73, 1.0]);
        assert_eq!(u.ppq, 960.0);
        assert_eq!(u.max_key_index, 127.0);
        assert_eq!(u.canvas_offset, [10.0, 20.0]);
        assert_eq!(u.canvas_size, [800.0, 600.0]);
        assert_eq!(u.time_signature_count, 2);
        assert_eq!(u.time_signatures[0], [0, 4, 4, 0]);
        assert_eq!(u.time_signatures[1], [3840, 3, 4, 0]);
        assert_eq!(u.time_signatures[2], [0, 0, 0, 0]);
    }

    /// 时间签名超过 16 条时截断，计数与内容一致。
    #[test]
    fn from_prepare_params_truncates_time_signatures_to_16() {
        let params = sample_params((0..20u32).map(|i| (i * 960, 4, 4)).collect());
        let u = GridCameraUniform::from_prepare_params(&params);

        assert_eq!(u.time_signature_count, 16);
        assert_eq!(u.time_signatures[15], [15 * 960, 4, 4, 0]);
        assert!(u.time_signatures.iter().all(|ts| ts[0] <= 15 * 960));
    }

    /// 默认值与旧 Builder 默认值等价：时间签名一条 4/4、计 1 条。
    #[test]
    fn default_matches_legacy_builder_defaults() {
        let u = GridCameraUniform::default();
        assert_eq!(u.viewport_size, [1.0, 1.0]);
        assert_eq!(u.zoom, [1.0, 1.0]);
        assert_eq!(u.ppq, 1920.0);
        assert_eq!(u.max_key_index, 127.0);
        assert_eq!(u.canvas_size, [800.0, 600.0]);
        assert_eq!(u.time_signature_count, 1);
        assert_eq!(u.time_signatures[0], [0, 4, 4, 0]);
        assert_eq!(u.time_signatures[1], [0, 0, 0, 0]);
    }
}
