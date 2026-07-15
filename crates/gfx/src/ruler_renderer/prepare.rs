//! 时间轴标尺渲染器 - prepare 逻辑
//!
//! 包含刻度实例生成（含缓存优化）与 GPU 数据上传。

use super::{
    GROWTH_FACTOR, RulerPrepareParams, RulerRenderer, RulerTickInstance, RulerViewportUniform,
};
use crate::gpu_resource_tracker;

impl RulerRenderer {
    /// 生成标尺刻度实例
    fn generate_tick_instances(&self, params: &RulerPrepareParams) -> Vec<RulerTickInstance> {
        let mut instances = Vec::new();

        // 计算可见时间范围
        let visible_tick_start = params.scroll_x / params.zoom_x;
        let visible_tick_end = (params.scroll_x + params.viewport_size.0) / params.zoom_x;

        // 小节线
        let measure_start = (visible_tick_start / params.ticks_per_measure as f32).floor() as u32;
        let measure_end = (visible_tick_end / params.ticks_per_measure as f32).ceil() as u32;

        for measure in measure_start..=measure_end {
            let tick = measure as f32 * params.ticks_per_measure as f32;
            let x = params.keyboard_width + tick * params.zoom_x - params.scroll_x;

            if x >= params.keyboard_width && x <= params.viewport_size.0 {
                instances.push(RulerTickInstance::new(
                    [x, 0.0],
                    [2.0, params.ruler_height],
                    self.measure_color,
                    0, // 小节线
                    tick,
                ));
            }
        }

        // 拍线
        let beat_start = (visible_tick_start / params.ticks_per_beat as f32).floor() as u32;
        let beat_end = (visible_tick_end / params.ticks_per_beat as f32).ceil() as u32;

        for beat in beat_start..=beat_end {
            let tick = beat as f32 * params.ticks_per_beat as f32;
            let x = params.keyboard_width + tick * params.zoom_x - params.scroll_x;

            // 跳过小节线位置
            if tick % params.ticks_per_measure as f32 == 0.0 {
                continue;
            }

            if x >= params.keyboard_width && x <= params.viewport_size.0 {
                instances.push(RulerTickInstance::new(
                    [x, params.ruler_height * 0.3],
                    [1.0, params.ruler_height * 0.7],
                    self.beat_color,
                    1, // 拍线
                    tick,
                ));
            }
        }

        instances
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
            || self.cache_ticks_per_beat != p.ticks_per_beat;

        if params_changed {
            self.cached_instances = self.generate_tick_instances(p);
            self.cache_scroll_x = p.scroll_x;
            self.cache_zoom_x = p.zoom_x;
            self.cache_viewport_width = p.viewport_size.0;
            self.cache_keyboard_width = p.keyboard_width;
            self.cache_ruler_height = p.ruler_height;
            self.cache_ticks_per_measure = p.ticks_per_measure;
            self.cache_ticks_per_beat = p.ticks_per_beat;
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
