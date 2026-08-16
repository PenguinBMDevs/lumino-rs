use super::chunk::MAX_CHUNKS;
use crate::note_renderer::NoteRenderer;

impl NoteRenderer {
    /// 绘制音符列表（带裁剪）
    ///
    /// 每 chunk 一次 `draw_indirect`：cull shader 已将各 chunk 的可见实例数
    /// 写入独立槽位（`indirect_buffer` offset = idx × slot_align），
    /// `first_instance = chunk_start` 使各 chunk 绘制到可见 buffer 的对应分区。
    pub fn draw<'r>(
        &'r self,
        render_pass: &mut wgpu::RenderPass<'r>,
        has_instances: bool,
        scissor_rect: Option<(u32, u32, u32, u32)>,
    ) {
        puffin::profile_function!();
        if !has_instances || self.last_upload_count == 0 {
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

        let count = self.last_upload_count as usize;
        let chunk_count = self.chunk_layout.chunk_count(count).min(MAX_CHUNKS);
        let bind_group_count = self.cull_bind_groups.len();
        for idx in 0..chunk_count.min(bind_group_count) {
            let offset = self.chunk_layout.chunk_offset_bytes(idx);
            render_pass.draw_indirect(&self.indirect_buffer, offset);
        }
    }
}
