//! 时间轴标尺渲染器 - prepare 逻辑
//!
//! 包含刻度实例生成（含缓存优化）与 GPU 数据上传。

use super::{
    GROWTH_FACTOR, RulerPrepareParams, RulerRenderer, RulerTickInstance, RulerViewportUniform,
};
use crate::gpu_resource_tracker;
use crate::grid::generate_ruler_instances;

impl RulerRenderer {
    /// 生成标尺刻度实例（支持拍号变化）
    fn generate_tick_instances(&self, params: &RulerPrepareParams) -> Vec<RulerTickInstance> {
        generate_ruler_instances(
            params.viewport_size.0,
            params.keyboard_width,
            params.ruler_height,
            params.scroll_x,
            params.zoom_x,
            params.ppq,
            &params.time_signatures,
        )
    }

    /// 准备渲染数据（带缓存优化）
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        params: &RulerPrepareParams,
    ) {
        puffin::profile_function!();

        let p = params;
        let params_changed = !self.cache_valid
            || self.cache_scroll_x != p.scroll_x
            || self.cache_zoom_x != p.zoom_x
            || self.cache_viewport_width != p.viewport_size.0
            || self.cache_keyboard_width != p.keyboard_width
            || self.cache_ruler_height != p.ruler_height
            || self.cache_ticks_per_measure != p.ticks_per_measure
            || self.cache_ticks_per_beat != p.ticks_per_beat
            || self.cache_time_signatures != p.time_signatures;

        if params_changed {
            self.cached_instances = self.generate_tick_instances(p);
            self.cache_scroll_x = p.scroll_x;
            self.cache_zoom_x = p.zoom_x;
            self.cache_viewport_width = p.viewport_size.0;
            self.cache_keyboard_width = p.keyboard_width;
            self.cache_ruler_height = p.ruler_height;
            self.cache_ticks_per_measure = p.ticks_per_measure;
            self.cache_ticks_per_beat = p.ticks_per_beat;
            self.cache_time_signatures.clone_from(&p.time_signatures);
            self.cache_valid = true;
        }

        let instances = &self.cached_instances;
        let instance_count = instances.len();

        if instance_count > self.capacity {
            let new_capacity = (self.capacity * GROWTH_FACTOR).max(instance_count);
            gpu_resource_tracker::sub_buffer(&self.instance_buffer);
            self.instance_buffer = Self::create_instance_buffer(device, new_capacity);
            self.capacity = new_capacity;
        }

        if instance_count > 0 {
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(instances));
        }

        let viewport_uniform = RulerViewportUniform::from_params(p);
        queue.write_buffer(
            &self.viewport_buffer,
            0,
            bytemuck::cast_slice(&[viewport_uniform]),
        );
    }
}
