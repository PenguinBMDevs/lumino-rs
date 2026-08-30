use super::ArrangementRenderer;

impl ArrangementRenderer {
    /// 绘制走带视图
    ///
    /// 绘制顺序：覆盖层背景（背景/lane/网格）→ 音符（常驻 GPU）→ 覆盖层前景
    /// （框选/ghost/指示线），保证音符位于 lane 之上、指示线位于音符之上。
    pub fn draw<'r>(&'r self, render_pass: &mut wgpu::RenderPass<'r>) {
        puffin::profile_scope!("arrangement::draw");
        if self.overlay_count == 0 && self.note_count == 0 {
            return;
        }

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);

        // ── 覆盖层背景（背景/lane/网格）──
        if self.overlay_back_len > 0 {
            render_pass.set_vertex_buffer(0, self.overlay_buffer.inner().slice(..));
            render_pass.draw(0..6, 0..self.overlay_back_len);
        }

        // ── 音符（常驻 GPU buffer，note-space，着色器定位）──
        if self.note_count > 0 {
            render_pass.set_vertex_buffer(0, self.note_buffer.inner().slice(..));
            render_pass.draw(0..6, 0..self.note_count);
        }

        // ── 覆盖层前景（框选/ghost/指示线）──
        let front_start = self.overlay_back_len;
        let front_end = self.overlay_count;
        if front_end > front_start {
            render_pass.set_vertex_buffer(0, self.overlay_buffer.inner().slice(..));
            render_pass.draw(0..6, front_start..front_end);
        }
    }
}
