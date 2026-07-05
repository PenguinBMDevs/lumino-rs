use bytemuck;

use crate::types::TileCoord;

use super::core_impl::HiResRenderer;
use super::uniform::HiResUniform;

impl HiResRenderer {
    /// 绘制可见贴图（在 render_pass 内调用）
    ///
    /// 若某个坐标存在临时脏区域覆层，则跳过该坐标的基础贴图绘制。
    /// 因为脏区域覆层已包含该音轨组当前完整状态（含删除后的音符），
    /// 直接替代基础贴图可避免旧音符透过覆层继续显示。
    pub fn render<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        visible_coords: &[TileCoord],
    ) {
        render_pass.set_pipeline(&self.pipeline);
        for coord in visible_coords {
            // 有临时覆层时跳过基础贴图——覆层会覆盖完整当前状态
            if self.dirty_overlays.contains_key(coord) {
                continue;
            }
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
        visible: &[(TileCoord, HiResUniform)],
    ) {
        for (coord, uniform) in visible {
            if let Some(gpu) = self.dirty_overlays.get(coord) {
                queue.write_buffer(&gpu.uniform_buffer, 0, bytemuck::bytes_of(uniform));
            }
        }
    }

    /// 绘制临时脏区域覆层（替代对应坐标的基础贴图）
    ///
    /// 覆层贴图包含该音轨组的完整当前状态，`render` 会跳过同一坐标的基础贴图，
    /// 因此此处直接绘制即可，无需再与基础贴图做 Alpha 叠加。
    pub fn render_dirty_overlays<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        visible_coords: &[TileCoord],
    ) {
        render_pass.set_pipeline(&self.pipeline);
        for coord in visible_coords {
            if let Some(gpu) = self.dirty_overlays.get(coord) {
                render_pass.set_bind_group(0, &gpu.bind_group, &[]);
                render_pass.draw(0..6, 0..1);
            }
        }
    }
}
