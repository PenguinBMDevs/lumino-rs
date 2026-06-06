use super::data::{ScissorRect, ViewportInfo};
use crate::host::Host;

impl Host {
    /// 收集视口信息
    pub(super) fn collect_viewport_info(&self) -> ViewportInfo {
        let phys = self.render_ctx.viewport.physical_size();

        if self.root.is_arrangement_mode() {
            let av = &self.root.arrangement_view.viewport;
            ViewportInfo {
                logical_size: self.render_ctx.viewport.logical_size(),
                physical_size: (phys.width, phys.height),
                scale: self.render_ctx.viewport.scale_factor(),
                canvas_offset: av.canvas_offset,
                canvas_size: av.canvas_size,
            }
        } else {
            let es = &self.root.editor.editor_state;
            ViewportInfo {
                logical_size: self.render_ctx.viewport.logical_size(),
                physical_size: (phys.width, phys.height),
                scale: self.render_ctx.viewport.scale_factor(),
                canvas_offset: es.canvas.offset,
                canvas_size: es.canvas.size,
            }
        }
    }

    /// 计算当前视口哈希
    pub(super) fn compute_current_viewport_hash(&self, viewport: &ViewportInfo) -> u64 {
        if self.root.is_arrangement_mode() {
            let av = &self.root.arrangement_view.viewport;
            let track_count = self.root.sidebar.tracks.len().max(1) as u16;
            crate::host::RenderCache::compute_viewport_hash(
                av.scroll_x,
                av.scroll_y,
                av.zoom_x,
                av.track_height,
                viewport.canvas_size.x,
                viewport.canvas_size.y,
                track_count,
            )
        } else {
            let v = &self.root.editor.editor_state.view;
            crate::host::RenderCache::compute_viewport_hash(
                v.scroll_x,
                v.scroll_y,
                v.zoom_x,
                v.zoom_y,
                viewport.canvas_size.x,
                viewport.canvas_size.y,
                v.visible_key_count,
            )
        }
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
        if self.root.is_arrangement_mode() {
            let av = &self.root.arrangement_view.viewport;
            let track_count = self.root.sidebar.tracks.len().max(1) as f32;
            // yinhe 风格：每个 track 的 lane 内显示 128 个 key 的缩略钢琴卷帘
            let keys_per_track = 128.0;
            let zoom_y = av.track_height / keys_per_track;
            let max_key_index = track_count * keys_per_track - 1.0;
            // scroll_y 从像素转换为逻辑 key 单位
            let scroll_y_logical = av.scroll_y / av.track_height * keys_per_track;
            lumino_gfx::CameraUniform::new(lumino_gfx::CameraParams {
                scroll: [av.scroll_x, scroll_y_logical],
                zoom: [av.zoom_x, zoom_y],
                viewport: [viewport.logical_size.width, viewport.logical_size.height],
                offset: [viewport.canvas_offset.x, viewport.canvas_offset.y],
                keyboard_width: 0.0,
                ruler_height: 0.0,
                max_key_index,
            })
        } else {
            let v = &self.root.editor.editor_state.view;
            lumino_gfx::CameraUniform::new(lumino_gfx::CameraParams {
                scroll: [v.scroll_x, v.scroll_y],
                zoom: [v.zoom_x, v.zoom_y],
                viewport: [viewport.logical_size.width, viewport.logical_size.height],
                offset: [viewport.canvas_offset.x, viewport.canvas_offset.y],
                keyboard_width: v.keyboard_width,
                ruler_height: v.ruler_height,
                max_key_index: (v.visible_key_count.saturating_sub(1)) as f32,
            })
        }
    }
}
