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

/// 卷帘内容区（选框/按钮的可见范围）：键盘列右侧、标尺下方的网格区域
fn content_bounds(editor: &Editor) -> Rectangle {
    let view = &editor.editor_state.view;
    let canvas = &editor.editor_state.canvas;
    Rectangle::new(
        Point::new(view.keyboard_width, view.ruler_height),
        Size::new(
            (canvas.size_x - view.keyboard_width).max(0.0),
            (canvas.size_y - view.ruler_height).max(0.0),
        ),
    )
}

/// 将区域框屏幕边界裁剪到卷帘内容区（视觉裁剪，数据不动）
///
/// 素材放置允许区域框 Y 向越界（`offset_keys` 可回绕 u8 key）、X 向超出
/// 歌曲范围，直接按原始边界绘制会让选框显示到键盘列/标尺/窗口之外。
/// 此处仅对**显示**求交集裁剪，区域框数据（tick/key 范围）保持不变。
///
/// 返回裁剪后的 `(left, right, top, bottom)`；选框完全在内容区外时返回 `None`。
fn clip_region_bounds(
    region: (f32, f32, f32, f32),
    content: Rectangle,
) -> Option<(f32, f32, f32, f32)> {
    let (left, right, top, bottom) = region;
    let rect = Rectangle::new(
        Point::new(left, top),
        Size::new((right - left).max(1.0), (bottom - top).max(1.0)),
    );
    let clipped = rect.intersection(&content)?;
    Some((
        clipped.x,
        clipped.x + clipped.width,
        clipped.y,
        clipped.y + clipped.height,
    ))
}

/// 计算区域框右侧悬浮按钮位置（垂直居中于区域框）
///
/// 按钮组钳制到卷帘内容区内：区域框移出/越界时按钮仍保持完整可见可点
/// （用户拖回区域框后按钮自动回到其右侧）。
pub fn i2m_button_rects(editor: &Editor) -> Option<I2mButtonRects> {
    let (_, right, top, bottom) = editor.i2m_region_screen_bounds()?;
    let content = content_bounds(editor);
    // 内容区高度不足以容纳单个按钮时（异常布局）不显示按钮
    if content.height < I2M_BUTTON_SIZE {
        return None;
    }
    let group_w = I2M_BUTTON_SIZE * 2.0 + I2M_BUTTON_SPACING;
    // 垂直中心钳制到内容区内，避免区域框 Y 向越界时按钮悬浮到键盘/标尺上方
    let center_y = ((top + bottom) * 0.5).clamp(
        content.y + I2M_BUTTON_SIZE * 0.5,
        content.y + content.height - I2M_BUTTON_SIZE * 0.5,
    );
    // 水平位置：优先区域框右侧，超出内容区右边缘时钳制到右边缘
    let x0 =
        (right + I2M_BUTTON_SPACING).min(content.x + content.width - group_w - I2M_BUTTON_SPACING);
    // 内容区过窄无法容纳按钮组时（异常布局）不显示按钮
    if x0 < content.x + I2M_BUTTON_SPACING {
        return None;
    }
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
    let content = content_bounds(editor);

    // 区域框（常驻显示）：超出卷帘内容区的部分强制裁剪（数据不动）
    if let Some(region) = editor.i2m_region_screen_bounds()
        && let Some((left, right, top, bottom)) = clip_region_bounds(region, content)
    {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造默认卷帘内容区（键盘列 120px + 标尺 24px，画布 800x600）
    fn default_content() -> Rectangle {
        Rectangle::new(Point::new(120.0, 24.0), Size::new(680.0, 576.0))
    }

    #[test]
    fn test_clip_region_bounds_fully_inside() {
        let clipped = clip_region_bounds((200.0, 500.0, 100.0, 300.0), default_content());
        assert_eq!(clipped, Some((200.0, 500.0, 100.0, 300.0)));
    }

    #[test]
    fn test_clip_region_bounds_top_overflow() {
        // 素材 Y 向越界（key 回绕/上移），选框顶部超出标尺 → 裁剪到内容区顶边
        let clipped = clip_region_bounds((200.0, 500.0, -50.0, 100.0), default_content());
        assert_eq!(clipped, Some((200.0, 500.0, 24.0, 100.0)));
    }

    #[test]
    fn test_clip_region_bounds_left_overflow() {
        // 素材 X 向越界（负 tick），选框左侧超出键盘列 → 裁剪到内容区左边
        let clipped = clip_region_bounds((50.0, 200.0, 100.0, 300.0), default_content());
        assert_eq!(clipped, Some((120.0, 200.0, 100.0, 300.0)));
    }

    #[test]
    fn test_clip_region_bounds_corner_overflow() {
        // 素材超出卷帘右/下边缘 → 裁剪到内容区右下角
        let clipped = clip_region_bounds((500.0, 900.0, 300.0, 700.0), default_content());
        assert_eq!(clipped, Some((500.0, 800.0, 300.0, 600.0)));
    }

    #[test]
    fn test_clip_region_bounds_fully_outside() {
        // 选框完全在内容区外（键盘列上方）→ 不绘制
        let clipped = clip_region_bounds((50.0, 100.0, -50.0, -10.0), default_content());
        assert_eq!(clipped, None);
    }

    #[test]
    fn test_clip_region_bounds_zero_content() {
        // 异常布局：内容区尺寸为 0 → 不绘制
        let empty = Rectangle::new(Point::new(120.0, 24.0), Size::new(0.0, 0.0));
        let clipped = clip_region_bounds((200.0, 500.0, 100.0, 300.0), empty);
        assert_eq!(clipped, None);
    }
}
