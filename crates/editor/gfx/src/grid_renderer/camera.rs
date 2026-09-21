//! 网格渲染器 — 相机 Uniform 构建实现

use super::*;

impl Default for GridCameraUniformBuilder {
    fn default() -> Self {
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
            time_signatures: vec![(0, 4, 4)],
        }
    }
}

impl GridCameraUniformBuilder {
    /// 设置视口尺寸
    pub fn viewport_size(mut self, width: f32, height: f32) -> Self {
        self.viewport_size = [width, height];
        self
    }

    /// 设置相机位置
    pub fn camera_pos(mut self, x: f32, y: f32) -> Self {
        self.camera_pos = [x, y];
        self
    }

    /// 设置缩放
    pub fn zoom(mut self, x: f32, y: f32) -> Self {
        self.zoom = [x, y];
        self
    }

    /// 设置边距（键盘宽度、标尺高度）
    pub fn margins(mut self, keyboard_width: f32, ruler_height: f32) -> Self {
        self.margins = [keyboard_width, ruler_height];
        self
    }

    /// 设置背景色
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

    /// 设置 PPQ
    pub fn ppq(mut self, ppq: f32) -> Self {
        self.ppq = ppq;
        self
    }

    /// 设置最大键索引
    pub fn max_key_index(mut self, max_key_index: f32) -> Self {
        self.max_key_index = max_key_index;
        self
    }

    /// 设置画布偏移
    pub fn canvas_offset(mut self, x: f32, y: f32) -> Self {
        self.canvas_offset = [x, y];
        self
    }

    /// 设置画布尺寸
    pub fn canvas_size(mut self, width: f32, height: f32) -> Self {
        self.canvas_size = [width, height];
        self
    }

    /// 设置拍号变化列表
    pub fn time_signatures(mut self, time_signatures: Vec<(u32, u8, u8)>) -> Self {
        self.time_signatures = time_signatures;
        self
    }

    /// 构建 [`GridCameraUniform`]
    pub fn build(self) -> GridCameraUniform {
        let count = self.time_signatures.len().min(16) as u32;
        let mut ts_arr = [[0u32; 4]; 16];
        for (i, (tick, num, den)) in self.time_signatures.iter().take(16).enumerate() {
            ts_arr[i] = [*tick, *num as u32, *den as u32, 0];
        }
        GridCameraUniform {
            viewport_size: self.viewport_size,
            camera_pos: self.camera_pos,
            zoom: self.zoom,
            margins: self.margins,
            color_bg: self.color_bg,
            color_bg_black_key: self.color_bg_black_key,
            color_bar: self.color_bar,
            color_beat: self.color_beat,
            color_half_beat: self.color_half_beat,
            color_grid: self.color_grid,
            color_key_line: self.color_key_line,
            ppq: self.ppq,
            max_key_index: self.max_key_index,
            canvas_offset: self.canvas_offset,
            canvas_size: self.canvas_size,
            time_signature_count: count,
            _padding: [0; 1],
            time_signatures: ts_arr,
        }
    }
}
