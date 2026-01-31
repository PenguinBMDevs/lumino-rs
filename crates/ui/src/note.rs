use iced_core::{Color, Point, Rectangle, Size, Theme};
use iced_widget::canvas::{Frame, Path};
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

}