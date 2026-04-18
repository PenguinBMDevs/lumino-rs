use super::data::{ScissorRect, ViewportInfo};
use crate::host::Host;

impl Host {
    /// 收集视口信息
    pub(super) fn collect_viewport_info(&self) -> ViewportInfo {
        let phys = self.viewport.physical_size();
        ViewportInfo {
            logical_size: self.viewport.logical_size(),
            physical_size: (phys.width, phys.height),
            scale: self.viewport.scale_factor(),
            canvas_offset: self.root.editor.canvas_offset,
            canvas_size: self.root.editor.canvas_size,
        }
    }

    /// 计算当前视口哈希
    pub(super) fn compute_current_viewport_hash(&self, viewport: &ViewportInfo) -> u64 {
        crate::host::RenderCache::compute_viewport_hash(
            self.root.editor.state.scroll_x,
            self.root.editor.state.scroll_y,
            self.root.editor.state.zoom_x,
            self.root.editor.state.zoom_y,
            viewport.canvas_size.x,
            viewport.canvas_size.y,
        )
    }

    /// 计算裁剪矩形
    pub(super) fn calculate_scissor_rect(&self, viewport: &ViewportInfo) -> ScissorRect {
        let (phys_w, phys_h) = viewport.physical_size;
        let scissor_x = ((viewport.canvas_offset.x * viewport.scale) as u32).min(phys_w);
        let scissor_y = ((viewport.canvas_offset.y * viewport.scale) as u32).min(phys_h);
        let scissor_width = ((viewport.canvas_size.x * viewport.scale) as u32)
            .min(phys_w.saturating_sub(scissor_x));
        let scissor_height = ((viewport.canvas_size.y * viewport.scale) as u32)
            .min(phys_h.saturating_sub(scissor_y));

        ScissorRect {
            x: scissor_x,
            y: scissor_y,
            width: scissor_width,
            height: scissor_height,
            has_valid_region: scissor_width > 0 && scissor_height > 0,
        }
    }

    /// 构建相机统一变量
    pub(super) fn build_camera_uniform(
        &self,
        viewport: &ViewportInfo,
    ) -> lumino_gfx::CameraUniform {
        lumino_gfx::CameraUniform::new(lumino_gfx::CameraParams {
            scroll: [
                self.root.editor.state.scroll_x,
                self.root.editor.state.scroll_y,
            ],
            zoom: [self.root.editor.state.zoom_x, self.root.editor.state.zoom_y],
            viewport: [viewport.logical_size.width, viewport.logical_size.height],
            offset: [viewport.canvas_offset.x, viewport.canvas_offset.y],
            keyboard_width: self.root.editor.state.keyboard_width,
            ruler_height: self.root.editor.state.ruler_height,
            max_key_index: (self.root.editor.state.visible_key_count.saturating_sub(1)) as f32,
        })
    }
}
