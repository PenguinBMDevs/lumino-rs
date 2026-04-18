//! GridRenderer 渲染方法

use super::camera::GridCameraUniform;
use super::instance::GridLineInstance;
use super::pipeline::GridRenderer;

#[expect(
    clippy::too_many_arguments,
    reason = "WGPU 渲染准备函数需要设备、队列、视口等多参数"
)]
impl GridRenderer {
    pub fn prepare(
        &mut self,
        _instances: &[GridLineInstance],
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        viewport_size: (f32, f32),
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
    ) {
        puffin::profile_function!();
        let viewport = GridCameraUniform::new(
            viewport_size.0,
            viewport_size.1,
            scroll_x,
            scroll_y,
            zoom_x,
            zoom_y,
            keyboard_width,
            ruler_height,
            color_bg,
            color_bg_black_key,
            color_bar,
            color_beat,
            color_half_beat,
            color_grid,
            color_key_line,
            ppq,
            max_key_index,
            canvas_offset_x,
            canvas_offset_y,
        );
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[viewport]));
    }

    pub fn draw<'r>(&'r self, render_pass: &mut wgpu::RenderPass<'r>, _instance_count: u32) {
        puffin::profile_function!();
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.draw(0..4, 0..1);
    }
}
