use iced_core::Size;
use iced_wgpu::graphics::Viewport;

use super::data::ViewportInfo;
use crate::host::Host;

/// 缩放因子兜底
///
/// `Viewport::with_physical_size` 直接做 `logical = physical / scale`
/// （`iced_graphics-0.14.0/src/viewport.rs`）⇒ 0 会得到 inf、NaN 会传染整条渲染链。
/// winit 的 `scale_factor()` 正常返回 1.0 以上，这里只是把异常环境挡在入口。
pub(crate) fn sanitize_scale_factor(scale: f64) -> f32 {
    // 区间用 `contains`：NaN 参与比较恒为 false，因此非有限值自然落到兜底分支
    if (0.5..=8.0).contains(&scale) {
        scale as f32
    } else {
        1.0
    }
}

/// 视口构造：**唯一**的「物理尺寸 + 缩放因子 → 视口」换算入口
///
/// 独立成纯函数是为了让尺寸/DPI 换算可单测——视口本身要真窗口才能构造，
/// 但它只是 `(物理尺寸, 缩放因子)` 的纯函数结果。
pub(crate) fn viewport_for(width: u32, height: u32, scale: f64) -> Viewport {
    Viewport::with_physical_size(Size::new(width, height), sanitize_scale_factor(scale))
}

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

#[cfg(test)]
mod tests {
    //! REF-001 G-1：物理尺寸 + 缩放因子 → 逻辑尺寸的换算。
    //!
    //! 视口本身要真窗口才能构造，但它是纯函数结果，因此这里锁定换算口径本身
    //! （这正是 DPI 切换时出错的那一环）。

    use super::*;

    #[test]
    fn viewport_derives_logical_size_from_physical_and_scale() {
        let viewport = viewport_for(1920, 1080, 1.5);

        assert_eq!(viewport.scale_factor(), 1.5, "缩放因子必须原样进视口");
        assert_eq!(viewport.physical_size(), Size::new(1920, 1080));
        assert_eq!(
            viewport.logical_size(),
            Size::new(1280.0, 720.0),
            "1920×1080 @150% 的逻辑尺寸是 1280×720"
        );
    }

    #[test]
    fn viewport_at_100_percent_is_identity() {
        let viewport = viewport_for(1440, 900, 1.0);

        assert_eq!(viewport.logical_size(), Size::new(1440.0, 900.0));
    }

    #[test]
    fn viewport_handles_fractional_scale() {
        let viewport = viewport_for(1600, 1000, 1.25);

        assert_eq!(viewport.scale_factor(), 1.25);
        assert_eq!(viewport.logical_size(), Size::new(1280.0, 800.0));
    }

    #[test]
    fn invalid_scale_falls_back_to_one() {
        // 0 / NaN / 负数都会让 `physical / scale` 得到 inf/NaN 或反向尺寸
        for scale in [0.0, f64::NAN, f64::INFINITY, -1.5] {
            let viewport = viewport_for(800, 600, scale);

            assert_eq!(viewport.scale_factor(), 1.0, "非法缩放因子必须兜底为 1.0");
            assert_eq!(viewport.logical_size(), Size::new(800.0, 600.0));
        }
    }
}
