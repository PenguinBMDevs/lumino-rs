//! CC 柱状条渲染器 — prepare 阶段逻辑

mod builder;
mod velocity;

pub use builder::build_cc_bar_instances;

use super::core::{CcBarInstance, CcBarRenderer, CcBarViewportUniform};

// ─── CC 面板布局常量（单一来源，各函数共用，禁止局部重定义） ────────
/// 工具栏高度（像素）
const TOOLBAR_HEIGHT: f32 = 28.0;

impl CcBarRenderer {
    /// 准备渲染数据
    ///
    /// `instances` — CC 柱状条实例列表（屏幕空间坐标）
    /// `viewport_size` — 视口尺寸（用于 NDC 转换）
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &[CcBarInstance],
        viewport_size: (f32, f32),
    ) {
        puffin::profile_function!();

        let instance_count = instances.len();

        // 扩容实例缓冲区（旧缓冲由 TrackedBuffer Drop 自动注销）
        if instance_count > self.capacity {
            let new_capacity = (self.capacity * Self::GROWTH_FACTOR).max(instance_count);
            self.instance_buffer = Self::create_instance_buffer(device, new_capacity);
            self.capacity = new_capacity;
        }

        // 上传实例数据
        if instance_count > 0 {
            queue.write_buffer(
                self.instance_buffer.inner(),
                0,
                bytemuck::cast_slice(instances),
            );
        }

        // 更新视口 uniform
        let viewport_uniform = CcBarViewportUniform::new(viewport_size.0, viewport_size.1);
        queue.write_buffer(
            self.viewport_buffer.inner(),
            0,
            bytemuck::cast_slice(&[viewport_uniform]),
        );
    }
}
