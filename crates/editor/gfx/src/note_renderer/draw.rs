use super::chunk::MAX_CHUNKS;
use crate::note_renderer::NoteRenderer;

impl NoteRenderer {
    /// 绘制音符列表（带裁剪）
    ///
    /// 每 chunk 一次 `draw_indirect`：cull shader 已将各 chunk 的可见实例数
    /// 写入独立槽位（`indirect_buffer` offset = idx × slot_align），
    /// 同时绑定对应 chunk 的可见实例切片（first_instance = 0，偏移由顶点缓冲切片提供）。
    pub fn draw<'r>(
        &'r self,
        render_pass: &mut wgpu::RenderPass<'r>,
        has_instances: bool,
        scissor_rect: Option<(u32, u32, u32, u32)>,
    ) {
        self.draw_with_pipeline(render_pass, has_instances, scissor_rect, false);
    }

    /// 纵向卷帘绘制（复用同缓冲，转置坐标，瀑布流风格的纵向流动）
    pub fn draw_vertical<'r>(
        &'r self,
        render_pass: &mut wgpu::RenderPass<'r>,
        has_instances: bool,
        scissor_rect: Option<(u32, u32, u32, u32)>,
    ) {
        self.draw_with_pipeline(render_pass, has_instances, scissor_rect, true);
    }

    /// VS cull 直绘（PREF-005）：每 chunk 一次普通 `draw(4 顶点 × chunk_len 实例)`。
    ///
    /// 与 [`NoteRenderer::draw`] 的差异：
    /// - 不读可见索引顶点缓冲（源索引来自 `@builtin(instance_index)`）；
    /// - 不读 indirect 参数（实例数由 CPU 按 chunk 给出）；
    /// - 可见性在 `vs_direct` 内判定 ⇒ **不需要 compute cull pass**。
    ///
    /// 关键性质：`instance_index` 升序 = 源索引升序 = `region_depth` 近→远序，
    /// 因此重叠片元天然按前→后到达，后来的远片元被 early-Z 拒绝（需不透明管线 +
    /// 开启 depth write）。未创建直绘管线的渲染器（预览层 / 导出无 depth 变体）
    /// 自动回退 cull + 可见索引路径。
    pub fn draw_direct<'r>(
        &'r self,
        render_pass: &mut wgpu::RenderPass<'r>,
        has_instances: bool,
        scissor_rect: Option<(u32, u32, u32, u32)>,
    ) {
        let Some(pipeline) = self.direct_pipeline.as_ref() else {
            // 该渲染器没有直绘管线：回退 cull + 可见索引路径，绝不静默不画
            self.draw_with_pipeline(render_pass, has_instances, scissor_rect, false);
            return;
        };
        puffin::profile_function!();
        if !has_instances || self.last_upload_count == 0 {
            return;
        }
        if let Some((x, y, width, height)) = scissor_rect {
            render_pass.set_scissor_rect(x, y, width, height);
        }
        render_pass.set_pipeline(pipeline);

        let count = self.last_upload_count as usize;
        let chunk_count = self.chunk_layout.chunk_count(count).min(MAX_CHUNKS);
        let bind_group_count = self.render_bind_groups.len();
        for idx in 0..chunk_count.min(bind_group_count) {
            let (_, chunk_len) = self.chunk_layout.chunk_range(count, idx);
            render_pass.set_bind_group(0, &self.render_bind_groups[idx], &[]);
            // 4 顶点（quad）× chunk_len 实例；chunk_start 由本 chunk 的 cull
            // uniform 槽位提供，深度基准与 cull 路径一致。
            render_pass.draw(0..4, 0..chunk_len as u32);
        }
    }

    pub(super) fn draw_with_pipeline<'r>(
        &'r self,
        render_pass: &mut wgpu::RenderPass<'r>,
        has_instances: bool,
        scissor_rect: Option<(u32, u32, u32, u32)>,
        is_vertical: bool,
    ) {
        puffin::profile_function!();
        if !has_instances || self.last_upload_count == 0 {
            return;
        }

        if let Some((x, y, width, height)) = scissor_rect {
            render_pass.set_scissor_rect(x, y, width, height);
        }

        let pipeline = if is_vertical {
            &self.vertical_pipeline
        } else {
            &self.pipeline
        };
        render_pass.set_pipeline(pipeline);

        let count = self.last_upload_count as usize;
        let chunk_count = self.chunk_layout.chunk_count(count).min(MAX_CHUNKS);
        let bind_group_count = self
            .cull_bind_groups
            .len()
            .min(self.render_bind_groups.len());
        // 可见索引缓冲每个元素 4 bytes（u32），与 visible_index_buffer_layout 一致
        let index_size = std::mem::size_of::<u32>() as u64;
        for idx in 0..chunk_count.min(bind_group_count) {
            let (chunk_start, chunk_len) = self.chunk_layout.chunk_range(count, idx);
            let chunk_offset = (chunk_start as u64) * index_size;
            let chunk_bytes = (chunk_len as u64) * index_size;
            render_pass.set_bind_group(0, &self.render_bind_groups[idx], &[]);
            render_pass.set_vertex_buffer(
                0,
                self.visible_instance_buffer
                    .inner()
                    .slice(chunk_offset..chunk_offset + chunk_bytes),
            );
            let offset = self.chunk_layout.chunk_offset_bytes(idx);
            render_pass.draw_indirect(self.indirect_buffer.inner(), offset);
        }
    }
}
