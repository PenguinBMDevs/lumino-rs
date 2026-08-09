//! 图片转 MIDI 区域框与悬浮按钮渲染
//!
//! 放置模式（Placing）下：
//! - 区域框常驻绘制（边框 + 淡填充），表示生成区域；
//! - 区域框右侧空白处绘制 √（确认）/ ×（取消）两个悬浮按钮，
//!   使用 30-40 像素圆角矩形，按钮内容为居中 SVG 图标纹理
//!   （iced canvas 绘制，wgpu 不参与这两个按钮的绘制）。

use crate::Editor;
use crate::grid::utils::{clip_region_bounds, content_bounds};
use iced_core::image::Handle;
use iced_core::{Image, Point, Rectangle, Size};
use iced_widget::canvas::{self, Geometry, Path, Stroke};
use lumino_editor_state::ImageToMidiMode;
use lumino_ui_core::Renderer;
use lumino_ui_core::constants::editor::{
    SELECTION_BOX_FILL_COLOR, SELECTION_BOX_STROKE_COLOR, SELECTION_BOX_STROKE_WIDTH,
};
use once_cell::sync::Lazy;

/// 悬浮按钮边长（用户要求 30-40 像素，取 36）
pub const I2M_BUTTON_SIZE: f32 = 36.0;
/// 按钮与区域框的间距
const I2M_BUTTON_SPACING: f32 = 8.0;
/// 按钮圆角半径
const I2M_BUTTON_RADIUS: f32 = 8.0;
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
    _theme: &lumino_ui_core::Theme,
    bounds: Rectangle,
) -> Option<Geometry<Renderer>> {
    if editor.editor_state.image_to_midi.mode != ImageToMidiMode::Placing {
        return None;
    }
    let mut frame = canvas::Frame::new(renderer, bounds.size());
    let mut has_content = false;
    let content = content_bounds(editor);

    // 区域框（常驻显示）：超出卷帘内容区的部分强制裁剪（数据不动）
    // 样式与其他框选框统一：灰色 3px 边框 + 比边框浅的灰色半透明填充
    if let Some(region) = editor.i2m_region_screen_bounds()
        && let Some((left, right, top, bottom)) = clip_region_bounds(region, content)
    {
        let rect = Rectangle::new(
            Point::new(left, top),
            Size::new((right - left).max(1.0), (bottom - top).max(1.0)),
        );
        let path = Path::rectangle(rect.position(), rect.size());
        frame.fill(&path, SELECTION_BOX_FILL_COLOR);
        let stroke = Stroke::default()
            .with_width(SELECTION_BOX_STROKE_WIDTH)
            .with_color(SELECTION_BOX_STROKE_COLOR);
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
    use lumino_editor_state::{ImageToMidiPreview, PreviewNote, RegionRect};

    /// 构造放置模式编辑器：素材预览 + 确认区域 (5000, 5300, key 110..120)
    ///
    /// 素材：单轨、orig_width 300、音符 (0, 60, 100)。
    /// 默认视图（128 键 × 20px、画布 800x600）下 key 110..120 的中心位于
    /// 内容区（y 24..600）内，用于验证按钮垂直居中的非钳制路径。
    fn placed_editor() -> Editor {
        let mut editor = Editor::new();
        let i2m = &mut editor.editor_state.image_to_midi;
        i2m.preview = Some(ImageToMidiPreview {
            tracks: vec![vec![PreviewNote {
                tick: 0.0,
                length: 100.0,
                key: 60,
            }]],
            orig_width: 300.0,
        });
        i2m.confirm_region(RegionRect::new(5000.0, 5300.0, 110, 120));
        // 画布尺寸（内容区/按钮钳制计算需要）
        editor.editor_state.canvas.size_x = 800.0;
        editor.editor_state.canvas.size_y = 600.0;
        editor
    }

    #[test]
    fn test_button_rects_inside_content_centered() {
        // 区域框中心在内容区内时：按钮垂直居中于区域框，且完全位于内容区内
        let editor = placed_editor();
        let btns = i2m_button_rects(&editor).expect("按钮应存在");
        let view = &editor.editor_state.view;
        let region = editor
            .editor_state
            .image_to_midi
            .region
            .expect("区域应存在");
        // 与 i2m_region_screen_bounds 相同的坐标语义
        let top = view.key_to_y(u16::from(region.key_hi));
        let bottom = view.key_to_y(u16::from(region.key_lo)) + view.zoom_y;
        let content = content_bounds(&editor);

        // 垂直居中于区域框
        let btn_center_y = btns.confirm.y + I2M_BUTTON_SIZE * 0.5;
        let region_center_y = (top + bottom) * 0.5;
        assert!(
            (btn_center_y - region_center_y).abs() < 1.0,
            "按钮应垂直居中于区域框（region center {region_center_y} vs button center {btn_center_y}）"
        );
        // 位于区域框右侧
        let right = view.tick_to_x(region.tick_end);
        assert!(btns.confirm.x >= right, "按钮应在区域框右侧");
        // 两个按钮均完整位于内容区内
        for rect in [btns.confirm, btns.cancel] {
            assert!(rect.x >= content.x);
            assert!(rect.y >= content.y);
            assert!(rect.x + rect.width <= content.x + content.width);
            assert!(rect.y + rect.height <= content.y + content.height);
        }
    }

    #[test]
    fn test_button_rects_clamped_when_region_outside() {
        // 区域框右移出内容区时：按钮钳制到内容区右边缘（保持完整可见可点）
        let mut editor = placed_editor();
        // 将区域框移到内容区右侧之外（tick 80000 远超出画布）
        let i2m = &mut editor.editor_state.image_to_midi;
        i2m.confirm_region(RegionRect::new(80000.0, 83000.0, 110, 120));
        let btns = i2m_button_rects(&editor).expect("按钮应存在");
        let content = content_bounds(&editor);

        // 按钮组钳制在内容区内，不越界悬浮
        for rect in [btns.confirm, btns.cancel] {
            assert!(rect.x >= content.x);
            assert!(rect.x + rect.width <= content.x + content.width + 0.5);
            assert!(rect.y >= content.y);
            assert!(rect.y + rect.height <= content.y + content.height + 0.5);
        }
    }

    #[test]
    fn test_button_rects_none_when_content_too_small() {
        // 异常布局：内容区高度不足以容纳按钮 → 不显示按钮
        let mut editor = placed_editor();
        editor.editor_state.canvas.size_y = 40.0;
        assert!(i2m_button_rects(&editor).is_none());
    }
}
