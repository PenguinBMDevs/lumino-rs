//! 亚像素档位：点图元直绘（PREF-004 P1）
//!
//! 何时使用：时间轴缩放小到「最细可画音符（吸附精度）也不足 1 像素」时，
//! quad 的形状与描边已无视觉意义，而每个音符仍要付「4 顶点 + 2 三角形」的
//! 图元固定成本。真机分段实测（RTX 2060 / 1920×1080 / 16M 音符全景）：
//! `cull 1.39ms + draw 11.97ms`——瓶颈在图元/光栅吞吐，不在 cull 带宽。
//!
//! 本路径做两件事：
//! 1. 主音符层改用 **PointList** 拓扑（每实例 1 顶点 1 点），
//! 2. **跳过 cull pass**——可见性判定移入 `vs_point`，省掉整趟全量读 + 可见索引写。
//!
//! 不影响 quad 路径：点管线独立（无顶点缓冲、入口 `vs_point`/`fs_point`），
//! 深度公式与 `vs_main` 逐字一致（`global_index = chunk_start + 源索引`），
//! 重叠音符的稳定裁决语义不回退。

use super::chunk::MAX_CHUNKS;
use crate::note_renderer::NoteRenderer;

impl NoteRenderer {
    /// 创建点图元直绘管线（入口 `vs_point` / `fs_point`，无顶点缓冲）。
    pub(super) fn create_point_pipeline(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        render_bind_group_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        needs_depth: bool,
    ) -> wgpu::RenderPipeline {
        crate::pipeline::RenderPipelineBuilder::new(device, "note_point_pipeline", shader)
            .vertex_entry("vs_point")
            .fragment_entry("fs_point")
            .bind_group(render_bind_group_layout)
            // 无顶点缓冲：源索引来自 @builtin(instance_index)
            .point_list()
            .alpha_blended_target(format)
            .depth_stencil(crate::constants::rendering::depth_stencil_state_for(
                needs_depth,
            ))
            .build()
    }

    /// 点模式准备：只更新相机 uniform，**不** dispatch cull。
    ///
    /// 与 [`NoteRenderer::prepare_pass`] 的差异：不重置 indirect buffer
    /// （点模式用普通 `draw`，不读 indirect 参数）、不跑 compute cull。
    pub fn prepare_direct(
        &self,
        camera: crate::note_renderer::types::CameraUniform,
        queue: &wgpu::Queue,
    ) {
        puffin::profile_function!();
        if self.last_upload_count == 0 {
            return;
        }
        queue.write_buffer(
            self.viewport_buffer.inner(),
            0,
            bytemuck::cast_slice(&[camera]),
        );
    }

    /// 点图元直绘：每 chunk 一次 `draw(1 顶点 × chunk_len 实例)`。
    ///
    /// 可见性由 `vs_point` 自判（不可见输出视口外点被裁剪），因此不需要
    /// cull 产出的可见索引列表，也不需要 `draw_indirect`。
    /// 未创建点管线的渲染器（预览 / 导出无 depth 变体）自动回退 quad 路径。
    pub fn draw_points<'r>(
        &'r self,
        render_pass: &mut wgpu::RenderPass<'r>,
        has_instances: bool,
        scissor_rect: Option<(u32, u32, u32, u32)>,
    ) {
        let Some(pipeline) = self.point_pipeline.as_ref() else {
            // 该渲染器没有点管线：回退到 quad + cull 输出路径，绝不静默不画
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
            // 1 顶点（点图元）× chunk_len 实例；`chunk_start` 由本 chunk 的
            // cull uniform 槽位提供，深度基准与 quad 路径一致。
            render_pass.draw(0..1, 0..chunk_len as u32);
        }
    }
}
