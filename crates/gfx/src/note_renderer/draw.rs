use crate::note_renderer::NoteRenderer;

impl NoteRenderer {
    /// 绘制音符列表（带裁剪）
    pub fn draw<'r>(
        &'r self,
        render_pass: &mut wgpu::RenderPass<'r>,
        has_instances: bool,
        scissor_rect: Option<(u32, u32, u32, u32)>,
    ) {
        puffin::profile_function!();
        if !has_instances {
            return;
        }

        // 设置裁剪矩形（限制绘制区域）
        if let Some((x, y, width, height)) = scissor_rect {
            render_pass.set_scissor_rect(x, y, width, height);
        }

        // 绑定管线并绘制
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.render_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.visible_instance_buffer.slice(..));

        // 使用间接绘制
        render_pass.draw_indirect(&self.indirect_buffer, 0);
    }
}
