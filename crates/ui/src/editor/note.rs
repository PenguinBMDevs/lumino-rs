use iced_core::{Color, Point, Rectangle, Size, Theme};
use iced_widget::canvas::{Frame, Path};
use crate::editor::state::ViewState;
use crate::Renderer;

// 音符结构体
pub struct Note {
    pub position: Point,
    pub size: Size,
    pub color: Color,
}

#[allow(dead_code)]
const POSITION: Point = Point::new(0.0, 0.0);
#[allow(dead_code)]
const SIZE: Size = Size::new(100.0, 20.0);
#[allow(dead_code)]
const RECT: Rectangle<f32> = Rectangle::new(POSITION, SIZE);

// 渲染到UI
impl Note {
    pub fn new(x: f32, y: f32, width: f32, height: f32, _color: Color, theme: &Theme) -> Self {
        let palette = theme.extended_palette().background;
        Self {
            position: Point::new(x, y),
            size: Size::new(width, height),
            color: palette.strong.color,
        }
    }
    /// 绘制音符，实际你需要在其他地方调用，例如我在钢琴卷帘上调用了一个
    pub fn draw(&self, frame: &mut Frame<Renderer>) {
        let path = Path::rectangle(self.position, self.size);
        frame.fill(&path, self.color);
    }

    pub fn from_mouse_position(mouse_pos: Point, view_state: &ViewState, theme: &Theme) -> Self {
        let palette = theme.extended_palette().background;
        // 鼠标坐标已经是 Canvas 局部坐标，直接使用
        // 需要考虑键盘宽度偏移和滚动偏移
        // screen_x = tick * zoom - scroll + keyboard_width
        // tick = (screen_x - keyboard_width + scroll) / zoom
        let adjusted_x = mouse_pos.x - view_state.keyboard_width + view_state.scroll_x;

        // 将像素坐标转换为tick坐标，然后按精度对齐
        let tick_x = adjusted_x / view_state.zoom_x;
        let snapped_tick_x = (tick_x / view_state.snap_precision).round() * view_state.snap_precision;

        // 计算对齐后的屏幕坐标，并对齐到物理像素边界
        // snapped_x = snapped_tick * zoom - scroll + keyboard_width
        let snapped_x = (snapped_tick_x * view_state.zoom_x - view_state.scroll_x + view_state.keyboard_width).round();

        // Y坐标向上贴合（取下面的网格线），并对齐到物理像素边界
        let snapped_y = (mouse_pos.y / view_state.zoom_y).floor() * view_state.zoom_y;
        let pixel_aligned_y = snapped_y.round();

        // 音符长度和高度也对齐到物理像素，避免亚像素渲染导致的视觉偏差
        let note_width = (view_state.default_note_length * view_state.zoom_x).round();
        let note_height = view_state.zoom_y.round();

        Self::new(
            snapped_x,         // 像素对齐的X坐标
            pixel_aligned_y,   // 像素对齐的Y坐标
            note_width,        // 像素对齐的宽度
            note_height,       // 像素对齐的高度
            palette.strong.color,
            theme,
        )
    }
}
