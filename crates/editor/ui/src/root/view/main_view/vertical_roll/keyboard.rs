//! 纵向卷帘键盘 Canvas（编辑区底部，键沿 X 轴铺开）

use iced_core::mouse;
use iced_core::{Color, Point, Rectangle, Size};
use iced_widget::canvas::{self, Frame, Program, Stroke};
use lumino_gfx::grid::is_black_key;

use crate::{Message, Renderer, Theme};
use iced_wgpu::Geometry;
use lumino_ui_editor::grid::theme::ThemeExt;

/// 纵向卷帘键盘
///
/// 与横向卷帘左侧竖向键盘对称：横向用 `zoom_y/scroll_y` 把键映射到 Y 轴；
/// 纵向版把同一套 pitch 轴（`zoom_y/scroll_y`）转置到 X 轴，键条横向排列。
/// 配色复用横向键盘同款 `ThemeExt`（黑/白键、键盘底色、边框）。
pub struct VerticalKeyboardProgram {
    /// 可见键数
    pub visible_key_count: u16,
    /// 纵向缩放（Pixels per Key），转置后作为键条宽度
    pub zoom_y: f32,
    /// 垂直滚动（pixel），转置后作为键条水平偏移
    pub scroll_y: f32,
    /// 左侧标尺宽度（转置后作为键盘 X 轴起始留白）
    pub ruler_width: f32,
}

impl Program<Message, Theme, Renderer> for VerticalKeyboardProgram {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        _renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(_renderer, bounds.size());

        // 键盘底色
        frame.fill_rectangle(
            Point::ORIGIN,
            bounds.size(),
            theme.keyboard_background_color(),
        );

        let max_key_index = (self.visible_key_count - 1) as f32;
        let key_height = bounds.height;

        for i in 0..self.visible_key_count {
            let keynum = i as isize;
            let world_x = (max_key_index - keynum as f32) * self.zoom_y;
            let screen_x = world_x - self.scroll_y + self.ruler_width;

            if screen_x + self.zoom_y >= self.ruler_width && screen_x <= bounds.width {
                let is_black = is_black_key(keynum);
                let base_color = if is_black {
                    theme.black_key_color()
                } else {
                    theme.white_key_color()
                };
                // 256 键扩展区（128-255）颜色微调：高亮系压暗、暗色系提亮
                let key_color = if i >= 128 {
                    let (r, g, b) = if theme.is_light() {
                        (
                            (base_color.r * 0.85).max(0.0),
                            (base_color.g * 0.85).max(0.0),
                            (base_color.b * 0.85).max(0.0),
                        )
                    } else {
                        (
                            (base_color.r * 1.15).min(1.0),
                            (base_color.g * 1.15).min(1.0),
                            (base_color.b * 1.15).min(1.0),
                        )
                    };
                    Color::from_rgba(r, g, b, base_color.a)
                } else {
                    base_color
                };

                let key_rect = Rectangle::new(
                    Point::new(screen_x, 0.0),
                    Size::new(self.zoom_y, key_height),
                );
                let key_path =
                    iced_widget::canvas::Path::rectangle(key_rect.position(), key_rect.size());
                frame.fill(&key_path, key_color);
                frame.stroke(
                    &key_path,
                    Stroke::default()
                        .with_width(1.0)
                        .with_color(theme.border_color()),
                );

                // 音符名称标签（横向键条较窄，仅白键标注音名以便定位）
                if !is_black {
                    let label = canvas::Text {
                        content: note_label(i as u8),
                        position: Point::new(screen_x + self.zoom_y / 2.0, key_height / 2.0),
                        max_width: self.zoom_y,
                        line_height: iced_core::text::LineHeight::Relative(1.0),
                        size: iced_core::Pixels(9.0),
                        color: theme.text_color(),
                        font: iced_core::Font::DEFAULT,
                        align_x: iced_core::alignment::Horizontal::Center.into(),
                        align_y: iced_core::alignment::Vertical::Center,
                        shaping: iced_core::text::Shaping::Basic,
                    };
                    frame.fill_text(label);
                }
            }
        }

        vec![frame.into_geometry()]
    }
}

/// 简化音符标签（取音名 + 八度，如 C4）；仅用于纵向键盘定位提示。
fn note_label(key: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let name = NAMES[(key % 12) as usize];
    let octave = (key / 12).saturating_sub(1);
    format!("{name}{octave}")
}
