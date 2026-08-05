use super::data::ViewportInfo;
use crate::host::Host;

impl Host {
    /// 收集视口信息
    pub(super) fn collect_viewport_info(&self) -> ViewportInfo {
        let logical_size = self.render_ctx.viewport.logical_size();

        if self.root.is_arrangement_mode() {
            // 音轨总览模式下，Canvas 位置 = sidebar 宽度 + track_list 宽度 + 上方 toolbar
            const TRACK_LIST_WIDTH: f32 = 160.0;
            const STATUSBAR_HEIGHT: f32 = 20.0;
            const TITLEBAR_HEIGHT: f32 = 30.0;
            const H_SCROLLBAR_HEIGHT: f32 = 20.0;
            const V_SCROLLBAR_WIDTH: f32 = 12.0;
            let sidebar_width = self.root.sidebar.width() as f32;
            let toolbar_height = self.root.toolbar.height();
            // 非 macOS 平台有自定义标题栏（30px），需计入偏移以保证与左侧 TrackListCanvas 对齐
            let titlebar_offset = if cfg!(target_os = "macos") {
                0.0
            } else {
                TITLEBAR_HEIGHT
            };
            let canvas_offset = iced_core::Point::new(
                sidebar_width + TRACK_LIST_WIDTH,
                toolbar_height + titlebar_offset,
            );
            let canvas_size = iced_core::Point::new(
                (logical_size.width - sidebar_width - TRACK_LIST_WIDTH - V_SCROLLBAR_WIDTH)
                    .max(1.0),
                (logical_size.height
                    - toolbar_height
                    - STATUSBAR_HEIGHT
                    - H_SCROLLBAR_HEIGHT
                    - titlebar_offset)
                    .max(1.0),
            );
            ViewportInfo {
                canvas_offset,
                canvas_size,
            }
        } else {
            let es = &self.root.editor.editor_state;
            ViewportInfo {
                canvas_offset: iced_core::Point::new(es.canvas.offset_x, es.canvas.offset_y),
                canvas_size: iced_core::Point::new(es.canvas.size_x, es.canvas.size_y),
            }
        }
    }
}
