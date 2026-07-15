//! 钢琴键盘绘制

use super::theme::ThemeExt;
use super::utils::{is_key_dark, note_name};
use crate::Editor;
use iced_core::{Point, Rectangle, Size, alignment};
use iced_widget::canvas::{Frame, Geometry, Path, Stroke, Text};
use lumino_ui_constants::editor::KEY_LABEL_FONT_SIZE;
use lumino_ui_core::Renderer;

/// 绘制钢琴键盘到 Geometry（用于 Canvas 绘制）
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

/// 绘制钢琴键盘（左侧键位指示器）
///
/// 注意：此函数只绘制基础键盘，不包含洋葱皮颜色。
/// 洋葱皮颜色通过 `draw_onion_overlay` 独立绘制，避免触发 keyboard_cache 重建。
pub fn draw(
    editor: &Editor,
    frame: &mut Frame<Renderer>,
    bounds: Rectangle,
    theme: &lumino_ui_core::Theme,
) {
    let view = &editor.editor_state.view;
    let keyboard_width = view.keyboard_width;
    let ruler_height = view.ruler_height;
    let max_key_index = (view.visible_key_count - 1) as f32;

    // 绘制键盘区域背景（时间轴标尺下方）
    let keyboard_bg_rect = Rectangle::new(
        Point::new(0.0, ruler_height),
        Size::new(keyboard_width, bounds.height - ruler_height),
    );
    let keyboard_bg_path = Path::rectangle(keyboard_bg_rect.position(), keyboard_bg_rect.size());
    let bg_color = theme.keyboard_background_color();
    frame.fill(&keyboard_bg_path, bg_color);

    // 绘制每个琴键（基础颜色，不含洋葱皮）
    for i in 0..view.visible_key_count {
        let keynum = i as isize;
        let world_y = (max_key_index - keynum as f32) * view.zoom_y;
        let screen_y = world_y - view.scroll_y + ruler_height;

        if screen_y + view.zoom_y >= ruler_height && screen_y <= bounds.height {
            let is_black_key = is_key_dark(keynum);
            let base_color = if is_black_key {
                theme.black_key_color()
            } else {
                theme.white_key_color()
            };

            // 256键扩展区域（128-255）的颜色微调
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

            let key_rect = Rectangle::new(
                Point::new(0.0, screen_y),
                Size::new(keyboard_width, view.zoom_y),
            );
            let key_path = Path::rectangle(key_rect.position(), key_rect.size());
            frame.fill(&key_path, key_color);

            let border_stroke = Stroke::default()
                .with_width(1.0)
                .with_color(theme.border_color());
            frame.stroke(&key_path, border_stroke);

            // 绘制音符名称标签
            let label_text = note_name(i as u8);
            let label_color = theme.text_color();

            let label = Text {
                content: label_text,
                position: Point::new(keyboard_width / 2.0, screen_y + view.zoom_y / 2.0),
                max_width: keyboard_width,
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

/// 绘制洋葱皮颜色覆盖层（不使用缓存，每帧独立绘制）
///
/// 此函数在 keyboard_cache 之上叠加半透明的洋葱皮颜色。
/// 由于不使用缓存，即使每帧绘制也不会触发 keyboard geometry 重建。
pub fn draw_onion_overlay(
    editor: &Editor,
    renderer: &Renderer,
    bounds: Rectangle,
) -> Option<Geometry<Renderer>> {
    let key_colors = &editor.playback_key_colors;

    // 快速检查：如果没有洋葱皮颜色，直接返回 None
    if *key_colors == [0u8; 1024] {
        return None;
    }

    let mut frame = Frame::new(renderer, bounds.size());
    let view = &editor.editor_state.view;
    let keyboard_width = view.keyboard_width;
    let ruler_height = view.ruler_height;
    let max_key_index = (view.visible_key_count - 1) as f32;

    for i in 0..view.visible_key_count {
        let offset = (i as usize) * 4;
        if key_colors[offset + 3] == 0 {
            continue; // 无颜色，跳过
        }

        let keynum = i as isize;
        let world_y = (max_key_index - keynum as f32) * view.zoom_y;
        let screen_y = world_y - view.scroll_y + ruler_height;

        if screen_y + view.zoom_y >= ruler_height && screen_y <= bounds.height {
            let onion_color = iced_core::Color::from_rgba8(
                key_colors[offset],
                key_colors[offset + 1],
                key_colors[offset + 2],
                key_colors[offset + 3] as f32 / 255.0 * 0.6, // 60% 不透明度
            );

            let key_rect = Rectangle::new(
                Point::new(0.0, screen_y),
                Size::new(keyboard_width, view.zoom_y),
            );
            let key_path = Path::rectangle(key_rect.position(), key_rect.size());
            frame.fill(&key_path, onion_color);
        }
    }

    Some(frame.into_geometry())
}
