use super::data::{ScissorRect, ViewportInfo};
use crate::host::Host;

impl Host {
    /// 收集视口信息
    pub(super) fn collect_viewport_info(&self) -> ViewportInfo {
        let phys = self.render_ctx.viewport.physical_size();
        let logical_size = self.render_ctx.viewport.logical_size();

        if self.root.is_arrangement_mode() {
            // 音轨总览模式下，Canvas 位置是固定的（左侧 track_list + 上方 toolbar）
            // 使用估算值，因为 Canvas 的 bounds 更新可能滞后
            const TRACK_LIST_WIDTH: f32 = 160.0;
            const STATUSBAR_HEIGHT: f32 = 20.0;
            const TITLEBAR_HEIGHT: f32 = 30.0;
            let toolbar_height = self.root.toolbar.height();
            // 非 macOS 平台有自定义标题栏（30px），需计入偏移以保证与左侧 TrackListCanvas 对齐
            let titlebar_offset = if cfg!(target_os = "macos") {
                0.0
            } else {
                TITLEBAR_HEIGHT
            };
            let canvas_offset =
                iced_core::Point::new(TRACK_LIST_WIDTH, toolbar_height + titlebar_offset);
            const H_SCROLLBAR_HEIGHT: f32 = 20.0;
            let canvas_size = iced_core::Point::new(
                (logical_size.width - TRACK_LIST_WIDTH).max(1.0),
                (logical_size.height
                    - toolbar_height
                    - STATUSBAR_HEIGHT
                    - H_SCROLLBAR_HEIGHT
                    - titlebar_offset)
                    .max(1.0),
            );
            ViewportInfo {
                logical_size,
                physical_size: (phys.width, phys.height),
                scale: self.render_ctx.viewport.scale_factor(),
                canvas_offset,
                canvas_size,
            }
        } else {
            let es = &self.root.editor.editor_state;
            ViewportInfo {
                logical_size: self.render_ctx.viewport.logical_size(),
                physical_size: (phys.width, phys.height),
                scale: self.render_ctx.viewport.scale_factor(),
                canvas_offset: iced_core::Point::new(es.canvas.offset_x, es.canvas.offset_y),
                canvas_size: iced_core::Point::new(es.canvas.size_x, es.canvas.size_y),
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
        let scissor_x = ((viewport.canvas_offset.x * viewport.scale).max(0.0) as u32).min(phys_w);
        let scissor_y = ((viewport.canvas_offset.y * viewport.scale).max(0.0) as u32).min(phys_h);
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
            // yinhe 风格：y 坐标已经是像素值，zoom_y = 1.0
            let track_height = av.track_height;
            let key_height = track_height / 128.0;
            let max_key_index = track_count * track_height - key_height;
            lumino_gfx::CameraUniform::new(lumino_gfx::CameraParams {
                scroll: [av.scroll_x, av.scroll_y],
                zoom: [av.zoom_x, 1.0],
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
