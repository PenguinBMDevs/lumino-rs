use super::super::types::KeyboardViewportUniform;
use super::KeyboardRenderer;

impl KeyboardRenderer {
    /// 准备渲染数据
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        viewport_size: (f32, f32),
        keyboard_width: f32,
        ruler_height: f32,
        scroll_y: f32,
        zoom_y: f32,
        visible_key_count: u16,
    ) {
        puffin::profile_function!();
        // 生成琴键实例
        let instances = self.generate_key_instances(
            visible_key_count,
            keyboard_width,
            zoom_y,
            scroll_y,
            ruler_height,
        );

        let instance_count = instances.len();

        // 扩容检查
        if instance_count > self.capacity {
            let new_capacity = (self.capacity * Self::GROWTH_FACTOR).max(instance_count);
            self.instance_buffer = Self::create_instance_buffer(device, new_capacity);
            self.capacity = new_capacity;
        }

        // 上传实例数据
        if instance_count > 0 {
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&instances));
        }

        // 更新视口 uniform
        let viewport_uniform = KeyboardViewportUniform::new(
            viewport_size.0,
            viewport_size.1,
            keyboard_width,
            ruler_height,
            scroll_y,
            zoom_y,
            visible_key_count,
        );
        queue.write_buffer(
            &self.viewport_buffer,
            0,
            bytemuck::cast_slice(&[viewport_uniform]),
        );
    }

    /// 执行渲染
    pub fn draw(&self, render_pass: &mut wgpu::RenderPass, instance_count: u32) {
        puffin::profile_function!();
        if instance_count == 0 {
            return;
        }

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        render_pass.draw(0..4, 0..instance_count);
    }
}
