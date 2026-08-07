//! 图片转 MIDI 区域框与悬浮按钮渲染
//!
//! 放置模式（Placing）下：
//! - 区域框常驻绘制（边框 + 淡填充），表示生成区域；
//! - 区域框右侧空白处绘制 √（确认）/ ×（取消）两个悬浮按钮，
//!   使用 30-40 像素圆角矩形，按钮内容为居中 SVG 图标纹理
//!   （iced canvas 绘制，wgpu 不参与这两个按钮的绘制）。

use crate::Editor;
use iced_core::image::Handle;
use iced_core::{Image, Point, Rectangle, Size};
use iced_widget::canvas::{self, Geometry, Path, Stroke};
use lumino_editor_state::ImageToMidiMode;
use lumino_ui_core::Renderer;
use once_cell::sync::Lazy;

/// 悬浮按钮边长（用户要求 30-40 像素，取 36）
pub const I2M_BUTTON_SIZE: f32 = 36.0;
/// 按钮与区域框的间距
const I2M_BUTTON_SPACING: f32 = 8.0;
/// 按钮圆角半径
const I2M_BUTTON_RADIUS: f32 = 8.0;
/// 区域框描边宽度
const I2M_REGION_STROKE: f32 = 2.0;
/// 图标占按钮边长的比例（留白保证图标居中不贴边）
const I2M_ICON_INSET_RATIO: f32 = 0.25;

/// √ 确认按钮图标（首次访问时由 SVG 光栅化，之后复用句柄 → iced_wgpu 纹理缓存命中）
static CONFIRM_ICON: Lazy<Handle> = Lazy::new(|| {
    build_icon_handle(include_bytes!(
        "../../../../resources/icons/toolbar/confirm-check.svg"
    ))
});
/// × 取消按钮图标
static CANCEL_ICON: Lazy<Handle> = Lazy::new(|| {
    build_icon_handle(include_bytes!(
        "../../../../resources/icons/toolbar/cancel-cross.svg"
    ))
});

/// 将内置 SVG 光栅化为图像句柄；失败时回退为空纹理并记录错误
fn build_icon_handle(svg: &[u8]) -> Handle {
    match lumino_ui_core::resources::icon::svg_handle(svg, 32) {
        Ok(handle) => handle,
        Err(e) => {
            tracing::error!("加载 i2m 悬浮按钮图标失败: {e}");
            Handle::from_rgba(1, 1, vec![0, 0, 0, 0])
        }
    }
}

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
            &CONFIRM_ICON,
            iced_core::Color::from_rgb8(46, 125, 50),
        );
        draw_button(
            &mut frame,
            btns.cancel,
            &CANCEL_ICON,
            iced_core::Color::from_rgb8(198, 40, 40),
        );
        has_content = true;
    }

    if has_content {
        Some(frame.into_geometry())
    } else {
        None
    }
}

/// 绘制单个悬浮按钮（圆角矩形 + 居中 SVG 图标）
fn draw_button(
    frame: &mut canvas::Frame<Renderer>,
    rect: Rectangle,
    icon: &Handle,
    bg: iced_core::Color,
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

    // 图标以按钮中心为基准等比内缩，保证水平/垂直居中
    let inset = rect.width * I2M_ICON_INSET_RATIO;
    let icon_bounds = Rectangle::new(
        Point::new(rect.x + inset, rect.y + inset),
        Size::new(rect.width - inset * 2.0, rect.height - inset * 2.0),
    );
    frame.draw_image(icon_bounds, Image::new(icon.clone()));
}
