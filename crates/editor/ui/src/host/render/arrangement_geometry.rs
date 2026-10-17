//! 工程走带区域几何：唯一权威的 canvas offset/size 计算。
//!
//! 走带视图的 WGPU 渲染区、顶部标尺、音轨列表、滚动条必须共享同一组
//! 布局常量。此前 `collect_viewport_info`（每帧渲染参数）与
//! `collect_render_data`（写回 arrangement_view.viewport）各写一份，
//! 新增 28px 标尺时任何一处漏改都会让 WGPU 网格与标尺错位。这里收口成
//! 纯函数并单测，两处只调用、不再各自维护常量。

use iced_core::Point;

/// 走带左侧音轨列表宽度
pub(crate) const TRACK_LIST_WIDTH: f32 = 160.0;
/// 底部状态栏高度
pub(crate) const STATUSBAR_HEIGHT: f32 = 20.0;
/// 底部水平滚动条高度
pub(crate) const H_SCROLLBAR_HEIGHT: f32 = 20.0;
/// 右侧垂直滚动条宽度（`ScrollbarWidget::vertical` 的实际宽度）
pub(crate) const V_SCROLLBAR_WIDTH: f32 = 20.0;

/// 走带区域几何
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ArrangementCanvasGeometry {
    /// WGPU 画布左上角（窗口逻辑坐标）
    pub offset: Point,
    /// WGPU 画布尺寸（逻辑像素）
    pub size: Point,
}

/// 计算走带区域几何。
///
/// `titlebar_offset` 仅非 macOS 非零（自制标题栏 30px）；`toolbar_height`
/// 由调用方传入（toolbar 高度是动态的）。
pub(crate) fn arrangement_canvas_geometry(
    logical_width: f32,
    logical_height: f32,
    sidebar_width: f32,
    toolbar_height: f32,
    titlebar_offset: f32,
) -> ArrangementCanvasGeometry {
    let ruler_height = lumino_ui_core::constants::editor::ARRANGEMENT_RULER_HEIGHT;
    ArrangementCanvasGeometry {
        offset: Point::new(
            sidebar_width + TRACK_LIST_WIDTH,
            toolbar_height + titlebar_offset + ruler_height,
        ),
        size: Point::new(
            (logical_width - sidebar_width - TRACK_LIST_WIDTH - V_SCROLLBAR_WIDTH).max(1.0),
            (logical_height
                - toolbar_height
                - STATUSBAR_HEIGHT
                - H_SCROLLBAR_HEIGHT
                - titlebar_offset
                - ruler_height)
                .max(1.0),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_ui_core::constants::editor::ARRANGEMENT_RULER_HEIGHT;

    #[test]
    fn test_geometry_includes_top_ruler() {
        let g = arrangement_canvas_geometry(1600.0, 900.0, 48.0, 72.0, 30.0);
        assert_eq!(g.offset, Point::new(48.0 + 160.0, 72.0 + 30.0 + 28.0));
        assert_eq!(
            g.size,
            Point::new(
                1600.0 - 48.0 - 160.0 - 20.0,
                900.0 - 72.0 - 20.0 - 20.0 - 30.0 - 28.0
            )
        );
        assert!(
            (g.size.y - (900.0 - 72.0 - 20.0 - 20.0 - 30.0 - ARRANGEMENT_RULER_HEIGHT)).abs()
                < f32::EPSILON,
            "标尺高度必须从画布高度中扣除"
        );
    }

    #[test]
    fn test_geometry_clamps_degenerate_size_to_one() {
        let g = arrangement_canvas_geometry(10.0, 10.0, 48.0, 72.0, 30.0);
        assert_eq!(g.size, Point::new(1.0, 1.0));
    }

    #[test]
    fn test_geometry_macos_has_no_titlebar_offset() {
        let g = arrangement_canvas_geometry(1600.0, 900.0, 48.0, 72.0, 0.0);
        assert_eq!(g.offset.y, 72.0 + 28.0);
    }
}
