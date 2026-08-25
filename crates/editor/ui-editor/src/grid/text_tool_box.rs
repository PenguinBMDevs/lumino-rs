//! 文字工具文本框与悬浮按钮渲染
//!
//! 激活文字工具并拉出框后，常驻绘制文本框（边框 + 淡填充）；
//! 框右侧绘制 √（确认）/ ×（取消）/ 模式切换三个悬浮按钮，
//! 视觉与曲线工具、图片转 MIDI 共用 `confirm_buttons` 模块。

use crate::Editor;
use crate::grid::confirm_buttons::{BUTTON_SIZE, CANCEL_ICON, CONFIRM_ICON, draw_button};
use crate::grid::utils::content_bounds;
use iced_core::{Color, Point, Rectangle, Size};
use iced_widget::canvas::{self, Geometry, Path, Stroke};
use lumino_ui_core::Renderer;
use lumino_ui_core::constants::editor::{
    SELECTION_BOX_FILL_COLOR, SELECTION_BOX_STROKE_COLOR, SELECTION_BOX_STROKE_WIDTH,
};

/// 按钮与文本框的间距
const TT_BUTTON_SPACING: f32 = 8.0;

/// 文字预览文字颜色（贴近白色，保证在文本框淡填充上可读）
const TEXT_PREVIEW_COLOR: Color = Color::from_rgba(0.92, 0.96, 1.0, 0.95);

/// 文字工具悬浮按钮矩形（画布坐标）
#[derive(Debug, Clone, Copy)]
pub struct TextToolButtonRects {
    /// √ 确认按钮
    pub confirm: Rectangle,
    /// × 取消按钮
    pub cancel: Rectangle,
    /// 模式切换按钮（正常 / key 范围合并）
    pub mode: Rectangle,
}

/// 计算文本框在屏幕上的矩形 (left, top, right, bottom)
///
/// 仅横向卷帘实现；纵向卷帘暂返回 `None`（后续补充转置映射）。
pub fn box_rect_screen(editor: &Editor) -> Option<(f32, f32, f32, f32)> {
    if !editor.editor_state.text_tool.active {
        return None;
    }
    if editor.editor_state.is_vertical_roll {
        return None;
    }
    let tt = &editor.editor_state.text_tool;
    let (tick_lo, tick_hi) = tt.normalized_ticks();
    let (key_lo, key_hi) = tt.normalized_keys();
    let view = &editor.editor_state.view;
    let left = view.tick_to_x(tick_lo);
    let right = view.tick_to_x(tick_hi);
    let top = view.key_to_y(key_hi);
    let bottom = view.key_to_y(key_lo) + view.zoom_y;
    Some((left, top, right, bottom))
}

/// 计算文本框右侧的悬浮按钮位置（垂直居中于文本框）
pub fn button_rects(editor: &Editor) -> Option<TextToolButtonRects> {
    let (_left, top, right, bottom) = box_rect_screen(editor)?;
    let content = content_bounds(editor);
    // 按钮组水平排布：确认 / 取消 / 模式
    let group_w = BUTTON_SIZE * 3.0 + TT_BUTTON_SPACING * 2.0;
    // 内容区过窄无法容纳按钮组时不显示
    if content.width < group_w + TT_BUTTON_SPACING * 2.0 {
        return None;
    }
    let center_y = ((top + bottom) * 0.5).clamp(
        content.y + BUTTON_SIZE * 0.5,
        content.y + content.height - BUTTON_SIZE * 0.5,
    );
    let x0 =
        (right + TT_BUTTON_SPACING).min(content.x + content.width - group_w - TT_BUTTON_SPACING);
    let y0 = center_y - BUTTON_SIZE * 0.5;
    let confirm = Rectangle::new(Point::new(x0, y0), Size::new(BUTTON_SIZE, BUTTON_SIZE));
    let cancel = Rectangle::new(
        Point::new(x0 + BUTTON_SIZE + TT_BUTTON_SPACING, y0),
        Size::new(BUTTON_SIZE, BUTTON_SIZE),
    );
    let mode = Rectangle::new(
        Point::new(x0 + (BUTTON_SIZE + TT_BUTTON_SPACING) * 2.0, y0),
        Size::new(BUTTON_SIZE, BUTTON_SIZE),
    );
    Some(TextToolButtonRects {
        confirm,
        cancel,
        mode,
    })
}

/// 绘制文本框（常驻）+ 悬浮按钮
pub fn draw(
    editor: &Editor,
    renderer: &Renderer,
    _theme: &lumino_ui_core::Theme,
    bounds: Rectangle,
) -> Option<Geometry<Renderer>> {
    let (left, top, right, bottom) = box_rect_screen(editor)?;
    let mut frame = canvas::Frame::new(renderer, bounds.size());
    let content = content_bounds(editor);

    // 文本框边框 + 淡填充
    let (cx0, cx1) = (left.max(content.x), right.min(content.x + content.width));
    let (cy0, cy1) = (top.max(content.y), bottom.min(content.y + content.height));
    if cx1 > cx0 && cy1 > cy0 {
        let rect = Rectangle::new(
            Point::new(cx0, cy0),
            Size::new((cx1 - cx0).max(1.0), (cy1 - cy0).max(1.0)),
        );
        let path = Path::rectangle(rect.position(), rect.size());
        frame.fill(&path, SELECTION_BOX_FILL_COLOR);
        let stroke = Stroke::default()
            .with_width(SELECTION_BOX_STROKE_WIDTH)
            .with_color(SELECTION_BOX_STROKE_COLOR);
        frame.stroke(&path, stroke);
    }

    // 文字预览：在框内显示「普通可读文字」（正常方向、按框尺寸自动缩放），
    // 便于用户核对输入内容。这里只做显示；音符生成仍由确认时的采样逻辑完成。
    {
        let tt = &editor.editor_state.text_tool;
        let text = tt.text.trim();
        if !text.is_empty() {
            let box_w = (right - left).max(1.0);
            let box_h = (bottom - top).max(1.0);
            let char_count = text.chars().count().max(1);
            // 估算字号：CJK 字符近似 1.0×size 宽、拉丁 0.5×size，取 0.6×size 折中，
            // 同时适配框高与框宽，避免溢出。
            let size_h = (box_h * 0.85).clamp(8.0, 400.0);
            let size_w = (box_w / (0.6 * char_count as f32)).clamp(8.0, 400.0);
            let size = size_h.min(size_w);
            let (cx0, cx1) = (left.max(content.x), right.min(content.x + content.width));
            let (cy0, cy1) = (top.max(content.y), bottom.min(content.y + content.height));
            if cx1 > cx0 && cy1 > cy0 {
                frame.fill_text(canvas::Text {
                    content: text.to_string(),
                    position: Point::new((cx0 + cx1) * 0.5, (cy0 + cy1) * 0.5),
                    max_width: cx1 - cx0,
                    line_height: iced_core::text::LineHeight::Relative(1.0),
                    size: iced_core::Pixels(size),
                    color: TEXT_PREVIEW_COLOR,
                    font: iced_core::Font::with_name(tt.font_family),
                    align_x: iced_core::alignment::Horizontal::Center.into(),
                    align_y: iced_core::alignment::Vertical::Center.into(),
                    shaping: iced_core::text::Shaping::Advanced,
                });
            }
        }
    }

    // 悬浮按钮
    if let Some(btns) = button_rects(editor) {
        draw_button(
            &mut frame,
            btns.confirm,
            &CONFIRM_ICON,
            Color::from_rgb8(46, 125, 50),
        );
        draw_button(
            &mut frame,
            btns.cancel,
            &CANCEL_ICON,
            Color::from_rgb8(198, 40, 40),
        );
        // 模式按钮：合并模式用蓝色高亮，正常模式用灰色；标签 M / N
        let merged = editor.editor_state.text_tool.mode.is_merged();
        let mode_bg = if merged {
            Color::from_rgb8(33, 118, 210)
        } else {
            Color::from_rgb8(120, 120, 120)
        };
        let path = Path::rounded_rectangle(
            btns.mode.position(),
            btns.mode.size(),
            iced_core::border::Radius::from(crate::grid::confirm_buttons::BUTTON_RADIUS),
        );
        frame.fill(&path, mode_bg);
        frame.fill_text(canvas::Text {
            content: if merged { "M" } else { "N" }.to_string(),
            position: Point::new(
                btns.mode.x + btns.mode.width * 0.5,
                btns.mode.y + btns.mode.height * 0.5 - 7.0,
            ),
            max_width: btns.mode.width,
            line_height: iced_core::text::LineHeight::Relative(1.0),
            size: iced_core::Pixels(14.0),
            color: Color::WHITE,
            font: iced_core::Font::DEFAULT,
            align_x: iced_core::alignment::Horizontal::Center.into(),
            align_y: iced_core::alignment::Vertical::Top,
            shaping: iced_core::text::Shaping::Basic,
        });
    }

    Some(frame.into_geometry())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_editor_state::text_tool::TextToolState;

    fn editor_with_box() -> Editor {
        let mut editor = Editor::new();
        editor.editor_state.text_tool.set_drag(0.0, 3840.0, 60, 64);
        editor.editor_state.text_tool.begin_editing(1920.0);
        editor.editor_state.canvas.size_x = 800.0;
        editor.editor_state.canvas.size_y = 600.0;
        editor
    }

    #[test]
    fn test_box_rect_screen_horizontal() {
        let editor = editor_with_box();
        let (l, t, r, b) = box_rect_screen(&editor).expect("应有框");
        assert!(r > l);
        assert!(b > t);
    }

    #[test]
    fn test_button_rects_present() {
        let editor = editor_with_box();
        let btns = button_rects(&editor).expect("应有按钮");
        // 三个按钮水平排布在框右侧
        assert!(btns.cancel.x > btns.confirm.x);
        assert!(btns.mode.x > btns.cancel.x);
    }

    #[test]
    fn test_no_rect_when_inactive() {
        let editor = Editor::new();
        assert!(box_rect_screen(&editor).is_none());
        // 避免未使用告警
        let _ = TextToolState::new();
    }
}
