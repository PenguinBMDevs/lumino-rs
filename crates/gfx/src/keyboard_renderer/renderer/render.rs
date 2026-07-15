use super::super::types::KeyboardViewportUniform;
use super::KeyboardRenderer;
use crate::gpu_resource_tracker;

/// 键盘渲染器准备参数
#[derive(Debug, Clone)]
pub struct KeyboardPrepareParams {
    pub viewport_size: (f32, f32),
    pub keyboard_width: f32,
    pub ruler_height: f32,
    pub scroll_y: f32,
    pub zoom_y: f32,
    pub visible_key_count: u16,
}

impl KeyboardRenderer {
    /// 准备渲染数据（带缓存优化）
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        params: &KeyboardPrepareParams,
    ) {
        puffin::profile_function!();

        let p = params;
        let params_changed = !self.cache_valid
            || self.cache_scroll_y != p.scroll_y
            || self.cache_zoom_y != p.zoom_y
            || self.cache_visible_key_count != p.visible_key_count
            || self.cache_keyboard_width != p.keyboard_width
            || self.cache_ruler_height != p.ruler_height;

        if params_changed {
            self.cached_instances = self.generate_key_instances(
                p.visible_key_count,
                p.keyboard_width,
                p.zoom_y,
                p.scroll_y,
                p.ruler_height,
            );
            self.cache_scroll_y = p.scroll_y;
            self.cache_zoom_y = p.zoom_y;
            self.cache_visible_key_count = p.visible_key_count;
            self.cache_keyboard_width = p.keyboard_width;
            self.cache_ruler_height = p.ruler_height;
            self.cache_valid = true;
        }

        let instances = &self.cached_instances;
        let instance_count = instances.len();

        if instance_count > self.capacity {
            let new_capacity = (self.capacity * Self::GROWTH_FACTOR).max(instance_count);
            gpu_resource_tracker::sub_buffer(&self.instance_buffer);
            self.instance_buffer = Self::create_instance_buffer(device, new_capacity);
            self.capacity = new_capacity;
        }

        if instance_count > 0 {
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(instances));
        }

        let viewport_uniform = KeyboardViewportUniform::from_params(p);
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
