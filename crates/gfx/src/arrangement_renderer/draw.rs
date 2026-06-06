use super::ArrangementRenderer;

impl ArrangementRenderer {
    /// 绘制走带视图
    pub fn draw<'r>(&'r self, render_pass: &mut wgpu::RenderPass<'r>) {
        if self.last_instance_count == 0 {
            return;
        }

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        // 6 顶点 (2 个三角形) × instance_count 个实例
        render_pass.draw(0..6, 0..self.last_instance_count);
    }
}
