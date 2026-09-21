//! 网格渲染器 — 准备与绘制实现

use super::*;

impl GridRenderer {
    /// 准备渲染数据（带缓存优化）
    pub fn prepare(&mut self, queue: &wgpu::Queue, params: &GridPrepareParams) {
        puffin::profile_function!();
        let viewport = GridCameraUniform::builder()
            .viewport_size(params.viewport_size.0, params.viewport_size.1)
            .camera_pos(params.scroll_x, params.scroll_y)
            .zoom(params.zoom_x, params.zoom_y)
            .margins(params.keyboard_width, params.ruler_height)
            .color_bg(params.color_bg)
            .color_bg_black_key(params.color_bg_black_key)
            .color_bar(params.color_bar)
            .color_beat(params.color_beat)
            .color_half_beat(params.color_half_beat)
            .color_grid(params.color_grid)
            .color_key_line(params.color_key_line)
            .ppq(params.ppq)
            .max_key_index(params.max_key_index)
            .canvas_offset(params.canvas_offset_x, params.canvas_offset_y)
            .canvas_size(params.canvas_size.0, params.canvas_size.1)
            .time_signatures(params.time_signatures.clone())
            .build();

        if self.cached_uniform.as_ref() != Some(&viewport) {
            queue.write_buffer(
                self.camera_buffer.inner(),
                0,
                bytemuck::cast_slice(&[viewport]),
            );
            self.cached_uniform = Some(viewport);
        }
    }

    /// 绘制网格线
    pub fn draw<'r>(&'r self, render_pass: &mut wgpu::RenderPass<'r>, _instance_count: u32) {
        puffin::profile_function!();
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        // 画一个全屏的四边形（4个顶点，使用 TriangleStrip）
        render_pass.draw(0..4, 0..1);
    }
}
