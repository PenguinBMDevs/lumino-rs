use crate::texture_waterfall::types::WaterfallTileCoord;

use super::core_impl::TextureWaterfallRenderer;
use super::uniform::TextureWaterfallUniform;

impl TextureWaterfallRenderer {
    /// 绘制可见贴图（在 render_pass 内调用）
    ///
    /// 基础贴图始终绘制。脏区域覆层在其之上用 Alpha 混合叠加，
    /// 透明像素（未修改的音轨）让基础贴图透出，不透明像素（已修改的音符）覆盖基础贴图。
    /// 避免在跨 track_group 全轨合并模式下，脏覆层（仅含修改音轨组数据）直接替代
    /// 基础贴图（含全部音轨数据）导致未修改音轨消失。
    pub fn render<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        visible_coords: &[WaterfallTileCoord],
        render_pass_has_depth: bool,
    ) {
        render_pass.set_pipeline(self.pipeline_for(render_pass_has_depth));
        for coord in visible_coords {
            // Always draw base tile; dirty_overlay renders on top with alpha blending.
            // Transparent overlay pixels let base tile show through for non-modified tracks.
            if let Some(gpu) = self.tiles.get(coord) {
                render_pass.set_bind_group(0, &gpu.bind_group, &[]);
                render_pass.draw(0..6, 0..1);
            }
        }
    }

    /// 准备临时脏区域覆层的 uniform
    pub fn prepare_dirty_overlays(
        &self,
        queue: &wgpu::Queue,
        visible: &[(WaterfallTileCoord, TextureWaterfallUniform)],
    ) {
        for (coord, uniform) in visible {
            if let Some(gpu) = self.dirty_overlays.get(coord) {
                queue.write_buffer(&gpu.uniform_buffer, 0, bytemuck::bytes_of(uniform));
            }
        }
    }

    /// 绘制临时脏区域覆层（Alpha 混合叠加在基础贴图之上）
    ///
    /// 利用管线 ALPHA_BLENDING：覆层中的透明像素让基础贴图透出，
    /// 不透明像素（已修改音轨）覆盖基础贴图。因删除导致的不透明像素残留
    /// 会在 `RegenerateTextureWaterfallTrack` 全轨合并后由新合并贴图替代。
    pub fn render_dirty_overlays<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        visible_coords: &[WaterfallTileCoord],
        render_pass_has_depth: bool,
    ) {
        render_pass.set_pipeline(self.pipeline_for(render_pass_has_depth));
        for coord in visible_coords {
            if let Some(gpu) = self.dirty_overlays.get(coord) {
                render_pass.set_bind_group(0, &gpu.bind_group, &[]);
                render_pass.draw(0..6, 0..1);
            }
        }
    }
}
