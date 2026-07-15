//! 演奏指示线渲染

use crate::Editor;
use iced_core::{Point, Rectangle};
use iced_widget::canvas::{Frame, Geometry, Path, Stroke};
use lumino_ui_constants::editor as editor_constants;
use lumino_ui_core::Renderer;

/// 绘制演奏指示线
pub fn draw(editor: &Editor, renderer: &Renderer, bounds: Rectangle) -> Geometry<Renderer> {
    let mut frame = Frame::new(renderer, bounds.size());

    // 获取演奏指示线的屏幕 X 坐标（考虑自动滚动模式）
    let keyboard_width = editor.editor_state.view.keyboard_width;
    let view_x = editor
        .get_playback_indicator_screen_x()
        .unwrap_or(keyboard_width);

    // 如果指示线位置在钢琴键盘区域内（左侧）或超出画布范围，则不绘制
    let canvas_width = bounds.width;
    if view_x < keyboard_width || view_x > canvas_width {
        return frame.into_geometry();
    }

    // 计算绘制区域（从标尺底部到画布底部）
    let start_y = 0.0;
    let end_y = bounds.height;

    // 指示线颜色：鲜艳的红色
    let indicator_color = iced_core::Color::from_rgb(1.0, 0.2, 0.2);

    // 绘制垂直线
    let line_path = Path::line(Point::new(view_x, start_y), Point::new(view_x, end_y));
    frame.stroke(
        &line_path,
        Stroke::default()
            .with_width(editor_constants::PLAYBACK_INDICATOR_WIDTH)
            .with_color(indicator_color),
    );

    // 绘制顶部倒三角形（▼）
    let triangle_size = editor_constants::PLAYBACK_INDICATOR_TRIANGLE_SIZE;
    let triangle_path = Path::new(|builder| {
        // 三角形顶点（从上往下）
        let top_left = Point::new(view_x - triangle_size / 2.0, start_y);
        let top_right = Point::new(view_x + triangle_size / 2.0, start_y);
        let bottom = Point::new(view_x, start_y + triangle_size);

        builder.move_to(top_left);
        builder.line_to(top_right);
        builder.line_to(bottom);
        builder.close();
    });

    // 填充三角形
    frame.fill(&triangle_path, indicator_color);

    frame.into_geometry()
}
