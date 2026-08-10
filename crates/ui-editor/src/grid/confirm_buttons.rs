//! 悬浮 √× 确认/取消按钮共享绘制
//!
//! i2m 区域框与曲线工具直线共用同一套按钮视觉：
//! 30-40 像素圆角矩形 + 居中 SVG 图标（iced canvas 绘制，wgpu 不参与）。

use iced_core::image::Handle;
use iced_core::{Image, Point, Rectangle, Size};
use iced_widget::canvas::{self, Path, Stroke};
use lumino_ui_core::Renderer;
use once_cell::sync::Lazy;

/// 悬浮按钮边长（用户要求 30-40 像素，取 36）
pub const BUTTON_SIZE: f32 = 36.0;
/// 按钮圆角半径
pub const BUTTON_RADIUS: f32 = 8.0;
/// 图标占按钮边长的比例（留白保证图标居中不贴边）
pub const ICON_INSET_RATIO: f32 = 0.25;

/// √ 确认按钮图标（首次访问时由 SVG 光栅化，之后复用句柄 → iced_wgpu 纹理缓存命中）
pub(crate) static CONFIRM_ICON: Lazy<Handle> = Lazy::new(|| {
    build_icon_handle(include_bytes!(
        "../../../../resources/icons/toolbar/confirm-check.svg"
    ))
});
/// × 取消按钮图标
pub(crate) static CANCEL_ICON: Lazy<Handle> = Lazy::new(|| {
    build_icon_handle(include_bytes!(
        "../../../../resources/icons/toolbar/cancel-cross.svg"
    ))
});

/// 将内置 SVG 光栅化为图像句柄；失败时回退为空纹理并记录错误
fn build_icon_handle(svg: &[u8]) -> Handle {
    match lumino_ui_core::resources::icon::svg_handle(svg, 32) {
        Ok(handle) => handle,
        Err(e) => {
            tracing::error!("加载悬浮按钮图标失败: {e}");
            Handle::from_rgba(1, 1, vec![0, 0, 0, 0])
        }
    }
}

/// 绘制单个悬浮按钮（圆角矩形 + 居中 SVG 图标）
pub fn draw_button(
    frame: &mut canvas::Frame<Renderer>,
    rect: Rectangle,
    icon: &Handle,
    bg: iced_core::Color,
) {
    let path = Path::rounded_rectangle(
        rect.position(),
        rect.size(),
        iced_core::border::Radius::from(BUTTON_RADIUS),
    );
    frame.fill(&path, bg);
    let stroke = Stroke::default()
        .with_width(1.0)
        .with_color(iced_core::Color::from_rgba(1.0, 1.0, 1.0, 0.6));
    frame.stroke(&path, stroke);

    // 图标以按钮中心为基准等比内缩，保证水平/垂直居中
    let inset = rect.width * ICON_INSET_RATIO;
    let icon_bounds = Rectangle::new(
        Point::new(rect.x + inset, rect.y + inset),
        Size::new(rect.width - inset * 2.0, rect.height - inset * 2.0),
    );
    frame.draw_image(icon_bounds, Image::new(icon.clone()));
}
