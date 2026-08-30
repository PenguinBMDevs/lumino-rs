use super::ArrangementRenderer;

impl ArrangementRenderer {
    /// 绘制走带视图
    ///
    /// 绘制顺序：覆盖层背景（背景/lane/网格）→ 音符（复用常驻 GPU 缓冲，分音轨分段）→
    /// 覆盖层前景（框选/ghost/指示线），保证音符位于 lane 之上、指示线位于音符之上。
    pub fn draw<'r>(&'r self, render_pass: &mut wgpu::RenderPass<'r>) {
        puffin::profile_scope!("arrangement::draw");
        if self.overlay_count == 0 && self.note_segments.is_empty() {
            return;
        }

        // ── 覆盖层背景（背景/lane/网格）──
        if self.overlay_back_len > 0 {
            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, &self.bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.overlay_buffer.inner().slice(..));
            render_pass.draw(0..6, 0..self.overlay_back_len);
        }

        // ── 音符（复用钢琴卷帘常驻 GPU 缓冲，按可见音轨分段 draw）──
        if !self.note_segments.is_empty() {
            render_pass.set_pipeline(&self.note_pipeline);
            render_pass.set_bind_group(0, &self.note_bind_group, &[]);
            for &(offset, len) in &self.note_segments {
                if len == 0 {
                    continue;
                }
                let start_byte = (offset as u64) * std::mem::size_of::<crate::NoteInstance>() as u64;
                let end_byte =
                    ((offset + len) as u64) * std::mem::size_of::<crate::NoteInstance>() as u64;
                render_pass.set_vertex_buffer(
                    0,
                    self.note_source.slice(start_byte..end_byte),
                );
                render_pass.draw(0..4, 0..len);
            }
        }

        // ── 覆盖层前景（框选/ghost/指示线）──
        let front_start = self.overlay_back_len;
        let front_end = self.overlay_count;
        if front_end > front_start {
            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, &self.bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.overlay_buffer.inner().slice(..));
            render_pass.draw(0..6, front_start..front_end);
        }
    }
}
