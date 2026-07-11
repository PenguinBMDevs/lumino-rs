//! 钢琴键盘绘制

use super::theme::ThemeExt;
use super::utils::{is_key_dark, note_name};
use crate::Renderer;
use crate::constants::editor::KEY_LABEL_FONT_SIZE;
use crate::editor::Editor;
use iced_core::{Point, Rectangle, Size, alignment};
use iced_widget::canvas::{Frame, Geometry, Path, Stroke, Text};

/// 绘制钢琴键盘到 Geometry（用于 Canvas 绘制）
pub fn draw_to_geometry(
    editor: &Editor,
    renderer: &Renderer,
    bounds: Rectangle,
    theme: &crate::Theme,
) -> Geometry<Renderer> {
    let mut frame = Frame::new(renderer, bounds.size());
    draw(editor, &mut frame, bounds, theme);
    frame.into_geometry()
}

/// 绘制钢琴键盘（左侧键位指示器）
pub fn draw(editor: &Editor, frame: &mut Frame<Renderer>, bounds: Rectangle, theme: &crate::Theme) {
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

    // 播放期间的洋葱皮琴键颜色映射（key → RGBA）
    let key_colors = &editor.playback_key_colors;

    // 绘制每个琴键
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
            // 亮色模式加深，暗色模式变浅
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

            // 播放期间：如果该 key 有洋葱皮音符正在发声，叠加洋葱皮颜色
            let final_color = if let Some(&onion_rgba) = key_colors.get(&i) {
                let onion_color = iced_core::Color::from_rgba8(
                    onion_rgba[0],
                    onion_rgba[1],
                    onion_rgba[2],
                    onion_rgba[3] as f32 / 255.0,
                );
                // 混合：洋葱皮颜色以 60% 不透明度叠加在基础琴键颜色之上
                blend_colors(key_color, onion_color, 0.6)
            } else {
                key_color
            };

            let key_rect = Rectangle::new(
                Point::new(0.0, screen_y),
                Size::new(keyboard_width, view.zoom_y),
            );
            let key_path = Path::rectangle(key_rect.position(), key_rect.size());
            frame.fill(&key_path, final_color);

            let border_stroke = Stroke::default()
                .with_width(1.0)
                .with_color(theme.border_color());
            frame.stroke(&key_path, border_stroke);

            // 绘制音符名称标签（亮色模式=黑色文字，暗色模式=白色文字）
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

/// 将两个颜色按指定比例混合（alpha-aware 正面合成）
///
/// `overlay_alpha` 控制覆盖层颜色的不透明度（0.0~1.0）。
/// 结果 = base × (1 - overlay_alpha × overlay.a) + overlay × overlay_alpha × overlay.a
fn blend_colors(
    base: iced_core::Color,
    overlay: iced_core::Color,
    overlay_alpha: f32,
) -> iced_core::Color {
    let oa = overlay.a * overlay_alpha;
    let inv = 1.0 - oa;
    iced_core::Color::from_rgba(
        (base.r * inv + overlay.r * oa).clamp(0.0, 1.0),
        (base.g * inv + overlay.g * oa).clamp(0.0, 1.0),
        (base.b * inv + overlay.b * oa).clamp(0.0, 1.0),
        base.a.max(overlay.a),
    )
}
