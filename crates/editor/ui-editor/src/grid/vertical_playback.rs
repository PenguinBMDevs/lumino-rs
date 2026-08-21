//! 纵向卷帘播放指示线绘制（水平红线，时间轴在 Y）
//!
//! 从 `vertical_program.rs` 抽出（文件长度纪律）：指示线渲染职责独立。

use crate::Editor;
use iced_core::{Point, Rectangle};
use iced_widget::canvas::{Frame, Path, Stroke};
use lumino_ui_core::Renderer;
use lumino_ui_core::constants::editor::{
    PLAYBACK_INDICATOR_TRIANGLE_SIZE, PLAYBACK_INDICATOR_WIDTH,
};

/// 绘制纵向播放指示线（头部在键盘顶部，向上时间递增）
pub fn draw(
    editor: &Editor,
    renderer: &Renderer,
    bounds: Rectangle,
) -> Option<iced_widget::canvas::Geometry<Renderer>> {
    let view = &editor.editor_state.view;
    let keyboard_h = view.keyboard_width;
    // 纵向隐藏横向标尺：网格从顶部 0 开始至键盘顶部
    if bounds.height <= keyboard_h {
        return None;
    }
    let grid_top = 0.0;
    let grid_bottom = bounds.height - keyboard_h;

    // 计算播放指示线 Y（纵向：时间轴在 Y，头部在键盘顶部，向上递增）
    let indicator_y = grid_bottom - editor.playback_position * view.zoom_x + view.scroll_x;

    if indicator_y < grid_top || indicator_y > grid_bottom {
        return None;
    }

    let mut frame = Frame::new(renderer, bounds.size());
    let indicator_color = iced_core::Color::from_rgb(1.0, 0.2, 0.2);
    let line_path = Path::line(
        Point::new(0.0, indicator_y),
        Point::new(bounds.width, indicator_y),
    );
    frame.stroke(
        &line_path,
        Stroke::default()
            .with_width(PLAYBACK_INDICATOR_WIDTH)
            .with_color(indicator_color),
    );
    // 左侧三角形指示
    let tri = PLAYBACK_INDICATOR_TRIANGLE_SIZE;
    let triangle_path = Path::new(|b| {
        let top = Point::new(0.0, indicator_y - tri / 2.0);
        let bottom = Point::new(0.0, indicator_y + tri / 2.0);
        let right = Point::new(tri, indicator_y);
        b.move_to(top);
        b.line_to(bottom);
        b.line_to(right);
        b.close();
    });
    frame.fill(&triangle_path, indicator_color);

    Some(frame.into_geometry())
}
