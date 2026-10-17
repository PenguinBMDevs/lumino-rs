//! 纵向卷帘小节号文本与边框（供 wgpu 网格接管后 Canvas 层调用）

use super::theme::ThemeExt;
use crate::Editor;
use iced_core::{Point, Rectangle};
use iced_widget::canvas::{Frame, Path, Stroke};
use lumino_ui_core::Renderer;

/// 标签相对小节线的上移量（字号 10 + 2px 间隙）。
///
/// 纵向时间轴从键盘顶线向上增长；tick 0 恰在 `grid_bottom` 上。旧实现把
/// 标签画在 `screen_y + 2.0`（键盘区内），网格里第一个可见标签变成第 2 小节。
/// 统一画在线上方，tick 0 的“1”才留在网格内（#146）。
const LABEL_LIFT: f32 = 12.0;

/// 小节号标签的顶部 y：线上方 `LABEL_LIFT`，且不得越过网格顶边。
fn label_top_for_line(screen_y: f32, grid_top: f32) -> f32 {
    (screen_y - LABEL_LIFT).max(grid_top)
}

/// 仅绘制小节号文本与边框（网格线已由 wgpu 绘制）
pub fn draw_labels(
    editor: &Editor,
    frame: &mut Frame<Renderer>,
    bounds: Rectangle,
    theme: &lumino_ui_core::Theme,
) {
    let view = &editor.editor_state.view;
    let ppq = view.ppq as f32;
    // 纵向隐藏横向标尺：网格从顶部 0 开始
    let keyboard_h = view.keyboard_width;
    if bounds.height <= keyboard_h || bounds.width <= 1.0 {
        return;
    }
    let grid_top = 0.0;
    let grid_bottom = bounds.height - keyboard_h;
    let grid_height = (grid_bottom - grid_top).max(0.0);
    let start_tick = view.scroll_x / view.zoom_x;
    let end_tick = (view.scroll_x + grid_height) / view.zoom_x;
    let ticks_per_measure = ppq * 4.0;
    if ticks_per_measure <= 0.0 {
        return;
    }
    let mut measure_tick = (start_tick / ticks_per_measure).ceil() * ticks_per_measure;
    let mut measure_no = (measure_tick / ticks_per_measure) as u32 + 1;
    let text_color = theme.text_color();
    while measure_tick < end_tick {
        let screen_y = grid_bottom - measure_tick * view.zoom_x + view.scroll_x;
        if screen_y >= grid_top && screen_y <= grid_bottom {
            let label = iced_widget::canvas::Text {
                content: measure_no.to_string(),
                position: Point::new(4.0, label_top_for_line(screen_y, grid_top)),
                max_width: 60.0,
                line_height: iced_core::text::LineHeight::Relative(1.0),
                size: iced_core::Pixels(10.0),
                color: text_color,
                font: lumino_ui_core::font::ui_font(),
                align_x: iced_core::alignment::Horizontal::Left.into(),
                align_y: iced_core::alignment::Vertical::Top,
                shaping: iced_core::text::Shaping::Basic,
            };
            frame.fill_text(label);
        }
        measure_tick += ticks_per_measure;
        measure_no += 1;
    }
    let border_stroke = Stroke::default()
        .with_width(1.0)
        .with_color(theme.border_color());
    let kb_top = Path::line(
        Point::new(0.0, grid_bottom),
        Point::new(bounds.width, grid_bottom),
    );
    frame.stroke(&kb_top, border_stroke);
    let top_border = Path::line(
        Point::new(0.0, grid_top),
        Point::new(bounds.width, grid_top),
    );
    frame.stroke(&top_border, border_stroke);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_label_top_for_first_measure_stays_inside_grid() {
        // tick 0 的线在 grid_bottom（如 600），标签必须画在网格内（< 600）。
        let y = label_top_for_line(600.0, 0.0);
        assert_eq!(y, 588.0);
        assert!(y < 600.0, "首个标签不得落到键盘区");
    }

    #[test]
    fn test_label_top_is_clamped_to_grid_top() {
        assert_eq!(label_top_for_line(5.0, 0.0), 0.0);
        assert_eq!(label_top_for_line(20.0, 10.0), 10.0);
    }
}
