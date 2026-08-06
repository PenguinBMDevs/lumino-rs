//! 图片转 MIDI 区域框与悬浮按钮渲染
//!
//! 放置模式（Placing）下：
//! - 区域框常驻绘制（边框 + 淡填充），表示生成区域；
//! - 区域框右侧空白处绘制 √（确认）/ ×（取消）两个悬浮按钮，
//!   使用 30-40 像素圆角矩形。

use crate::Editor;
use iced_core::{Point, Rectangle, Size};
use iced_widget::canvas::{self, Geometry, Path, Stroke, Text};
use lumino_editor_state::ImageToMidiMode;
use lumino_ui_core::Renderer;

/// 悬浮按钮边长（用户要求 30-40 像素，取 36）
pub const I2M_BUTTON_SIZE: f32 = 36.0;
/// 按钮与区域框的间距
const I2M_BUTTON_SPACING: f32 = 8.0;
/// 按钮圆角半径
const I2M_BUTTON_RADIUS: f32 = 8.0;
/// 区域框描边宽度
const I2M_REGION_STROKE: f32 = 2.0;

/// 悬浮按钮矩形（画布坐标）
#[derive(Debug, Clone, Copy)]
pub struct I2mButtonRects {
    /// √ 确认按钮
    pub confirm: Rectangle,
    /// × 取消按钮
    pub cancel: Rectangle,
}

/// 计算区域框右侧悬浮按钮位置（垂直居中于区域框）
pub fn i2m_button_rects(editor: &Editor) -> Option<I2mButtonRects> {
    let (_, right, top, bottom) = editor.i2m_region_screen_bounds()?;
    let center_y = (top + bottom) * 0.5;
    let x0 = right + I2M_BUTTON_SPACING;
    let y0 = center_y - I2M_BUTTON_SIZE * 0.5;
    let confirm = Rectangle::new(
        Point::new(x0, y0),
        Size::new(I2M_BUTTON_SIZE, I2M_BUTTON_SIZE),
    );
    let cancel = Rectangle::new(
        Point::new(x0 + I2M_BUTTON_SIZE + I2M_BUTTON_SPACING, y0),
        Size::new(I2M_BUTTON_SIZE, I2M_BUTTON_SIZE),
    );
    Some(I2mButtonRects { confirm, cancel })
}

/// 绘制区域框（常驻）+ √× 悬浮按钮
///
/// 仅在 `Placing` 阶段绘制；`Selecting` 阶段的框选矩形由
/// `selection_box::draw` 基于 `EditState::Selecting` 绘制。
pub fn draw(
    editor: &Editor,
    renderer: &Renderer,
    theme: &lumino_ui_core::Theme,
    bounds: Rectangle,
) -> Option<Geometry<Renderer>> {
    if editor.editor_state.image_to_midi.mode != ImageToMidiMode::Placing {
        return None;
    }
    let palette = theme.extended_palette();
    let mut frame = canvas::Frame::new(renderer, bounds.size());
    let mut has_content = false;

    // 区域框（常驻显示）
    if let Some((left, right, top, bottom)) = editor.i2m_region_screen_bounds() {
        let rect = Rectangle::new(
            Point::new(left, top),
            Size::new((right - left).max(1.0), (bottom - top).max(1.0)),
        );
        let path = Path::rectangle(rect.position(), rect.size());
        let fill = iced_core::Color {
            a: 0.12,
            ..palette.primary.weak.color
        };
        frame.fill(&path, fill);
        let stroke = Stroke::default()
            .with_width(I2M_REGION_STROKE)
            .with_color(palette.primary.strong.color);
        frame.stroke(&path, stroke);
        has_content = true;
    }

    // 悬浮按钮
    if let Some(btns) = i2m_button_rects(editor) {
        draw_button(
            &mut frame,
            btns.confirm,
            "\u{221A}",
            iced_core::Color::from_rgb8(46, 125, 50),
            iced_core::Color::from_rgb8(200, 255, 200),
        );
        draw_button(
            &mut frame,
            btns.cancel,
            "\u{00D7}",
            iced_core::Color::from_rgb8(198, 40, 40),
            iced_core::Color::from_rgb8(255, 200, 200),
        );
        has_content = true;
    }

    if has_content {
        Some(frame.into_geometry())
    } else {
        None
    }
}

/// 绘制单个悬浮按钮（圆角矩形 + 居中字符）
fn draw_button(
    frame: &mut canvas::Frame<Renderer>,
    rect: Rectangle,
    glyph: &str,
    bg: iced_core::Color,
    fg: iced_core::Color,
) {
    let path = Path::rounded_rectangle(
        rect.position(),
        rect.size(),
        iced_core::border::Radius::from(I2M_BUTTON_RADIUS),
    );
    frame.fill(&path, bg);
    let stroke = Stroke::default()
        .with_width(1.0)
        .with_color(iced_core::Color::from_rgba(1.0, 1.0, 1.0, 0.6));
    frame.stroke(&path, stroke);

    let text = Text {
        content: glyph.to_string(),
        position: Point::new(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5),
        max_width: rect.width,
        line_height: iced_core::text::LineHeight::Relative(1.0),
        size: iced_core::Pixels(24.0),
        color: fg,
        font: iced_core::Font::DEFAULT,
        align_x: iced_core::alignment::Horizontal::Center.into(),
        align_y: iced_core::alignment::Vertical::Center,
        shaping: iced_core::text::Shaping::Basic,
    };
    frame.fill_text(text);
}
