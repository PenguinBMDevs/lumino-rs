//! 网格渲染器 — 准备与绘制实现

use super::*;

impl GridRenderer {
    /// 准备渲染数据（带缓存优化）
    pub fn prepare(&mut self, queue: &wgpu::Queue, params: &GridPrepareParams) {
        self.binding.prepare(queue, params);
    }

    /// 绘制网格线
    pub fn draw<'r>(&'r self, render_pass: &mut wgpu::RenderPass<'r>, _instance_count: u32) {
        puffin::profile_function!();
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, self.binding.bind_group(), &[]);
        // 画一个全屏的四边形（4个顶点，使用 TriangleStrip）
        render_pass.draw(0..4, 0..1);
    }
}
