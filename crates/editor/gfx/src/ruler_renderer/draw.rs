//! 时间轴标尺渲染器 - 绘制方法

use super::RulerRenderer;

impl RulerRenderer {
    /// 返回当前缓存的实例数量。
    ///
    /// 调用方应使用本方法替代 `RenderParams.ruler_instances.len()`：
    /// 后者由 UI 线程每帧重新生成，与 GPU 端 `cached_instances` 在参数
    ///（如 `keyboard_width`）不一致时长度可能不同，作为 `instance_count`
    /// 传给 `draw` 会读到 buffer 末尾的垃圾数据。本方法直接返回 GPU 端
    /// 实际持有的实例数，保证 `draw(0..4, 0..instance_count)` 安全。
    pub fn instance_count(&self) -> usize {
        self.cached_instances.len()
    }

    /// 执行渲染
    pub fn draw(&self, render_pass: &mut wgpu::RenderPass, instance_count: u32) {
        puffin::profile_function!();
        if instance_count == 0 {
            return;
        }

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.instance_buffer.inner().slice(..));
        render_pass.draw(0..4, 0..instance_count);
    }
}
