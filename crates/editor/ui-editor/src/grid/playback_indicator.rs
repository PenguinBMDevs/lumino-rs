//! 演奏指示线渲染

use crate::Editor;
use iced_core::{Point, Rectangle};
use iced_widget::canvas::{Frame, Geometry, Path, Stroke};
use lumino_ui_core::Renderer;
use lumino_ui_core::constants::editor as editor_constants;

/// 绘制演奏指示线
///
/// `is_vertical` 为 true 时纵向卷帘把时间轴转置到 Y 方向，指示线改为
/// 横向红线（从左侧键盘顶到底部），并带朝右的三角形（指示时间增长方向）。
pub fn draw(
    editor: &Editor,
    renderer: &Renderer,
    bounds: Rectangle,
    is_vertical: bool,
) -> Geometry<Renderer> {
    let mut frame = Frame::new(renderer, bounds.size());

    // 指示线颜色：鲜艳的红色
    let indicator_color = iced_core::Color::from_rgb(1.0, 0.2, 0.2);
    let triangle_size = editor_constants::PLAYBACK_INDICATOR_TRIANGLE_SIZE;

    if is_vertical {
        let keyboard_height = lumino_core::view_state::DEFAULT_KEYBOARD_WIDTH;
        let view_y = editor
            .get_playback_indicator_screen_y()
            .unwrap_or(keyboard_height);

        // 指示线越过键盘留白（顶部）或超出画布高度则不绘制
        let canvas_height = bounds.height;
        if view_y < keyboard_height || view_y > canvas_height {
            return frame.into_geometry();
        }

        // 横向红线（从左键盘边到画布右缘）
        let start_x = 0.0;
        let end_x = bounds.width;
        let line_path = Path::line(Point::new(start_x, view_y), Point::new(end_x, view_y));
        frame.stroke(
            &line_path,
            Stroke::default()
                .with_width(editor_constants::PLAYBACK_INDICATOR_WIDTH)
                .with_color(indicator_color),
        );

        // 左侧朝右三角形（▼ 指向时间增长方向）
        let triangle_path = Path::new(|builder| {
            let top = Point::new(start_x, view_y - triangle_size / 2.0);
            let bottom = Point::new(start_x, view_y + triangle_size / 2.0);
            let right = Point::new(start_x + triangle_size, view_y);
            builder.move_to(top);
            builder.line_to(bottom);
            builder.line_to(right);
            builder.close();
        });
        frame.fill(&triangle_path, indicator_color);
    } else {
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

        // 绘制垂直线
        let line_path = Path::line(Point::new(view_x, start_y), Point::new(view_x, end_y));
        frame.stroke(
            &line_path,
            Stroke::default()
                .with_width(editor_constants::PLAYBACK_INDICATOR_WIDTH)
                .with_color(indicator_color),
        );

        // 绘制顶部倒三角形（▼）
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
    }

    frame.into_geometry()
}
