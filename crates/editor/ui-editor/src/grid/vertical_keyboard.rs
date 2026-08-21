//! 纵向卷帘钢琴键盘绘制（底部横向排列）
//!
//! 样式与横向卷帘 `keyboard.rs` 完全一致：复用同款主题色（白键/黑键/边框/文字）、
//! 同款 `is_key_dark` / `note_name` / 256 键扩展微调逻辑、边框与文字居中。
//! 仅布局转置：
//! - 横向：键沿 Y 轴竖直排列，位于左侧 `keyboard_width` 宽度区域
//! - 纵向：键沿 X 轴水平排列，位于底部 `VERTICAL_KEYBOARD_HEIGHT` 高度区域

use super::theme::ThemeExt;
use super::utils::{is_key_dark, note_name};
use crate::Editor;
use iced_core::{Point, Rectangle, Size, alignment};
use iced_widget::canvas::{Frame, Geometry, Path, Stroke, Text};
use lumino_ui_core::Renderer;
use lumino_ui_core::constants::editor::KEY_LABEL_FONT_SIZE;

/// 纵向卷帘底部键盘高度（像素）
///
/// 与横向 `DEFAULT_KEYBOARD_WIDTH = 120` 保持一致，保证横/纵向键盘厚度视觉统一。
/// 键宽随 `zoom_y` 拉伸，键盘高度固定为横向键盘宽度。
pub const VERTICAL_KEYBOARD_HEIGHT: f32 = 120.0;

/// 绘制纵向卷帘键盘到 Geometry（用于 Canvas 绘制）
pub fn draw_to_geometry(
    editor: &Editor,
    renderer: &Renderer,
    bounds: Rectangle,
    theme: &lumino_ui_core::Theme,
) -> Geometry<Renderer> {
    let mut frame = Frame::new(renderer, bounds.size());
    draw(editor, &mut frame, bounds, theme);
    frame.into_geometry()
}

/// 绘制纵向钢琴键盘（底部横向键位指示器）
///
/// 键沿 X 轴水平排列，底部对齐。黑白键颜色、256 键扩展微调、边框与标签
/// 均与横向 `keyboard::draw` 同款，仅坐标系转置：
/// `world_x = key * zoom_y` → `screen_x = world_x - scroll_y`
pub fn draw(
    editor: &Editor,
    frame: &mut Frame<Renderer>,
    bounds: Rectangle,
    theme: &lumino_ui_core::Theme,
) {
    let view = &editor.editor_state.view;
    // 键盘区域：贴底横条，宽度铺满，高度与横向键盘宽度保持一致
    let keyboard_h = view.keyboard_width;
    if bounds.height <= keyboard_h || bounds.width <= 1.0 {
        return;
    }
    let keyboard_bg_rect = Rectangle::new(
        Point::new(0.0, bounds.height - keyboard_h),
        Size::new(bounds.width, keyboard_h),
    );
    let keyboard_bg_path = Path::rectangle(keyboard_bg_rect.position(), keyboard_bg_rect.size());
    let bg_color = theme.keyboard_background_color();
    frame.fill(&keyboard_bg_path, bg_color);

    // 绘制每个琴键（基础颜色，不含洋葱皮）
    for i in 0..view.visible_key_count {
        let keynum = i as isize;
        let world_x = keynum as f32 * view.zoom_y;
        let screen_x = world_x - view.scroll_y;

        // 可视裁剪：键的 X 范围与键盘区域相交
        if screen_x + view.zoom_y < 0.0 || screen_x > bounds.width {
            continue;
        }

        let is_black_key = is_key_dark(keynum);
        let base_color = if is_black_key {
            theme.black_key_color()
        } else {
            theme.white_key_color()
        };

        // 256 键扩展区域（128-255）的颜色微调（与横向键盘一致）
        let key_color = if i >= 128 {
            let is_light = theme.is_light();
            if is_light {
                iced_core::Color::from_rgba(
                    (base_color.r * 0.85f32).max(0.0),
                    (base_color.g * 0.85f32).max(0.0),
                    (base_color.b * 0.85f32).max(0.0),
                    base_color.a,
                )
            } else {
                iced_core::Color::from_rgba(
                    (base_color.r * 1.15f32).min(1.0),
                    (base_color.g * 1.15f32).min(1.0),
                    (base_color.b * 1.15f32).min(1.0),
                    base_color.a,
                )
            }
        } else {
            base_color
        };

        // 键矩形：沿 X 排列，Y 贴底，宽度 = zoom_y，高度 = keyboard_h
        // 做 1px 裁剪避免越界：键超出 bounds 右边缘时裁剪宽度
        let visible_w = (view.zoom_y).min(bounds.width - screen_x).max(0.0);
        if visible_w < 0.5 {
            continue;
        }
        let key_rect = Rectangle::new(
            Point::new(screen_x, bounds.height - keyboard_h),
            Size::new(visible_w, keyboard_h),
        );
        let key_path = Path::rectangle(key_rect.position(), key_rect.size());
        frame.fill(&key_path, key_color);

        let border_stroke = Stroke::default()
            .with_width(1.0)
            .with_color(theme.border_color());
        frame.stroke(&key_path, border_stroke);

        // 绘制音符名称标签（仅当键宽足够时显示，避免文字重叠）
        if view.zoom_y >= 14.0 {
            let label_text = note_name(i as u8);
            let label_color = theme.text_color();
            let label = Text {
                content: label_text,
                position: Point::new(screen_x + visible_w / 2.0, bounds.height - keyboard_h / 2.0),
                max_width: visible_w,
                line_height: iced_core::text::LineHeight::Relative(1.0),
                size: iced_core::Pixels(KEY_LABEL_FONT_SIZE),
                color: label_color,
                font: iced_core::Font::DEFAULT,
                align_x: alignment::Horizontal::Center.into(),
                align_y: alignment::Vertical::Center,
                shaping: iced_core::text::Shaping::Basic,
            };
            frame.fill_text(label);
        }
    }
}

/// 绘制洋葱皮颜色覆盖层（纵向键盘版，不使用缓存，每帧独立绘制）
///
/// 逻辑与横向 `keyboard::draw_onion_overlay` 同源，仅坐标转置到 X 轴。
pub fn draw_onion_overlay(
    editor: &Editor,
    renderer: &Renderer,
    bounds: Rectangle,
) -> Option<Geometry<Renderer>> {
    let key_colors = &editor.playback_key_colors;

    if *key_colors == [0u8; 1024] {
        return None;
    }

    let mut frame = Frame::new(renderer, bounds.size());
    let view = &editor.editor_state.view;
    let keyboard_h = view.keyboard_width;
    if bounds.height <= keyboard_h {
        return None;
    }

    for i in 0..view.visible_key_count {
        let offset = (i as usize) * 4;
        if key_colors[offset + 3] == 0 {
            continue;
        }

        let world_x = i as f32 * view.zoom_y;
        let screen_x = world_x - view.scroll_y;

        if screen_x + view.zoom_y < 0.0 || screen_x > bounds.width {
            continue;
        }

        let visible_w = (view.zoom_y).min(bounds.width - screen_x).max(0.0);
        if visible_w < 0.5 {
            continue;
        }

        let onion_color = iced_core::Color::from_rgba8(
            key_colors[offset],
            key_colors[offset + 1],
            key_colors[offset + 2],
            key_colors[offset + 3] as f32 / 255.0 * 0.6,
        );

        let key_rect = Rectangle::new(
            Point::new(screen_x, bounds.height - keyboard_h),
            Size::new(visible_w, keyboard_h),
        );
        let key_path = Path::rectangle(key_rect.position(), key_rect.size());
        frame.fill(&key_path, onion_color);
    }

    Some(frame.into_geometry())
}
