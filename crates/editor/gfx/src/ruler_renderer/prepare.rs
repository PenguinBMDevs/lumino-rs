//! 时间轴标尺渲染器 - prepare 逻辑
//!
//! 包含刻度实例生成（含缓存优化）与 GPU 数据上传。

use super::{
    GROWTH_FACTOR, RulerPrepareParams, RulerRenderer, RulerTickInstance, RulerViewportUniform,
};
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
    ///
    /// 性能优化（火焰图分析 2026-08-05）：
    /// - 旧逻辑无论缓存是否命中都每帧 `queue.write_buffer`，播放时 22ms/帧
    /// - 新逻辑：缓存命中时跳过 instance buffer 上传，仅更新 viewport uniform
    /// - viewport uniform 体积极小（几十字节），write_buffer 开销可忽略
    ///
    /// scroll_x 容差优化：播放时 scroll_x 每帧微变（亚像素级），但标尺刻度位置
    /// 由 `tick * zoom_x - scroll_x` 计算，scroll_x 变化 1 像素以内时刻度线
    /// 仍在同一像素位置（浮点取整后无差异）。设 1.0 像素容差，避免亚像素
    /// 抖动触发实例重建。zoom_x 不加容差（缩放变化必须立即重建）。
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        params: &RulerPrepareParams,
    ) {
        puffin::profile_function!();

        const SCROLL_X_TOLERANCE: f32 = 1.0;

        let params_changed = !self.cache_valid
            || (self.cache_scroll_x - params.scroll_x).abs() > SCROLL_X_TOLERANCE
            || self.cache_zoom_x != params.zoom_x
            || self.cache_viewport_width != params.viewport_size.0
            || self.cache_keyboard_width != params.keyboard_width
            || self.cache_ruler_height != params.ruler_height
            || self.cache_ticks_per_measure != params.ticks_per_measure
            || self.cache_ticks_per_beat != params.ticks_per_beat
            || self.cache_time_signatures != params.time_signatures;

        if params_changed {
            self.cached_instances = self.generate_tick_instances(params);
            self.cache_scroll_x = params.scroll_x;
            self.cache_zoom_x = params.zoom_x;
            self.cache_viewport_width = params.viewport_size.0;
            self.cache_keyboard_width = params.keyboard_width;
            self.cache_ruler_height = params.ruler_height;
            self.cache_ticks_per_measure = params.ticks_per_measure;
            self.cache_ticks_per_beat = params.ticks_per_beat;
            self.cache_time_signatures
                .clone_from(&params.time_signatures);
            self.cache_valid = true;

            // 仅在实例变化时重建 + 上传 instance buffer
            let instances = &self.cached_instances;
            let instance_count = instances.len();

            if instance_count > self.capacity {
                let new_capacity = (self.capacity * GROWTH_FACTOR).max(instance_count);
                self.instance_buffer = Self::create_instance_buffer(device, new_capacity);
                self.capacity = new_capacity;
            }

            if instance_count > 0 {
                queue.write_buffer(
                    self.instance_buffer.inner(),
                    0,
                    bytemuck::cast_slice(instances),
                );
            }
        }

        // viewport uniform 每帧更新（体积极小，含 scroll_x 等视口参数）
        let viewport_uniform = RulerViewportUniform::from_params(params);
        queue.write_buffer(
            self.viewport_buffer.inner(),
            0,
            bytemuck::cast_slice(&[viewport_uniform]),
        );
    }
}
