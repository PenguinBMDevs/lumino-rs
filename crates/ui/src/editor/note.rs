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

    pub fn from_mouse_position(mouse_pos: Point, _scroll_x: f32, _scroll_y: f32, theme: &Theme) -> Self {
        let palette = theme.extended_palette().background;
        // 鼠标坐标已经是 Canvas 局部坐标，直接使用
        // Y坐标向上贴合（取下面的网格线）
        let snspped_x = (mouse_pos.x / 20.0).floor() * 20.0; // -----|
        let snapped_y = (mouse_pos.y / 20.0).floor() * 20.0; // ---------->这里只是一个假设值，我们后期需要把这里的20.0根据实际缩放计算！（theme.zoom_x&y）
        Self::new(
            snspped_x,    // 对齐后的X坐标
            snapped_y,    // 对齐后的Y坐标
            100.0,
            20.0,
            palette.strong.color,
            theme,
        )
    }
}
