//! 文字工具文本框与悬浮按钮渲染
//!
//! 激活文字工具并拉出框后，常驻绘制文本框（边框 + 淡填充）；
//! 框右侧绘制 √（确认）/ ×（取消）/ 模式切换三个悬浮按钮，
//! 视觉与曲线工具、图片转 MIDI 共用 `confirm_buttons` 模块。
//!
//! **横向 / 纵向卷帘均支持**（2026-10 补齐纵向）：`box_rect_screen` 是文本框、按钮与
//! TextInput 覆盖层的唯一几何来源，纵向走转置映射（key→X、tick→Y 且 tick 越大越靠上）。
//! 字形**两轴的角色**（列→哪个逻辑轴）由 `GlyphGrid::from_state` 唯一决定，纵向把
//! advance 挂到 key 轴上——因此预览位图无需任何转置，铺进框矩形即为正立可读，且与
//! √ 生成的音符逐格重合。

use crate::Editor;
use crate::grid::confirm_buttons::{BUTTON_SIZE, CANCEL_ICON, CONFIRM_ICON, draw_button};
use crate::grid::utils::content_bounds;
use crate::interaction::text_tool::{GlyphGrid, rasterize_glyph_alpha};
use iced_core::image::{self, FilterMethod};
use iced_core::{Color, Image, Point, Rectangle, Size};
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
/// - 横向：X = tick、Y = key（key 越大越靠上）；
/// - 纵向（转置）：X = key、Y = tick，且 **tick 越大越靠上**
///   （与 `Editor::tick_to_y_vertical` 同一口径，见 `coords.rs`）。
///
/// 两个方向都必须给出几何：本函数是文本框、√×/模式按钮与 TextInput 覆盖层的**唯一**
/// 几何来源（三处都经 `box_rect_screen?` / `button_rects` 取用），任一方向返回 `None`
/// 都会让整条文字工具链路（预览 / 按钮 / 输入框）静默失效——纵向卷帘此前正是如此：
/// 工具可选、拉框有反应，但松手后无框、无按钮、无输入框，表现为"点了没反应"。
pub fn box_rect_screen(editor: &Editor) -> Option<(f32, f32, f32, f32)> {
    if !editor.editor_state.text_tool.active {
        return None;
    }
    let tt = &editor.editor_state.text_tool;
    let (tick_lo, tick_hi) = tt.normalized_ticks();
    let (key_lo, key_hi) = tt.normalized_keys();
    let view = &editor.editor_state.view;
    if editor.editor_state.is_vertical_roll {
        // key → X（key 越大越靠右，覆盖整行 key：右缘 = key_hi 的右边界）
        let left = editor.key_to_x_vertical(key_lo);
        let right = editor.key_to_x_vertical(key_hi) + view.zoom_y;
        // tick → Y（tick 越大越靠上：tick_hi 在上、tick_lo 在下）
        let top = editor.tick_to_y_vertical(tick_hi);
        let bottom = editor.tick_to_y_vertical(tick_lo);
        return Some((left, top, right, bottom));
    }
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
    // Conductor 音轨（track 0）：整工具不可用，不绘制文本框与悬浮按钮
    if !editor.text_tool_allowed() {
        return None;
    }
    // 与曲线/形状/画刷同一条可见性规则：只在**文字工具或鼠标工具**下渲染
    // （见 `Editor::pending_preview_visible`）——文本框与文字不因切换工具被丢弃，
    // 但也不该在铅笔/橡皮等音符编辑工具下当浮层干扰编辑。
    if !editor.pending_preview_visible(lumino_message::Tool::Text) {
        return None;
    }
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

    // 文字预览：直接渲染「生成音符用的同一份栅格」，保证预览与最终放置的音符完全一致
    // （同为底部对齐、同被拉伸铺满框，方向与音符一一对应）——看到的就是生成的。
    //
    // 横向 / 纵向都不做任何转置：栅格的两轴角色由 `GlyphGrid` 决定，位图 x 已是屏幕 X、
    // 位图 y 已是屏幕 Y（纵向 = 列→key→向右、行→tick→向下），故原样铺进框矩形即正立可读。
    {
        let tt = &editor.editor_state.text_tool;
        let text = tt.text.trim();
        if !text.is_empty() {
            let grid = GlyphGrid::from_state(
                tt,
                editor.editor_state.view.snap_precision,
                editor.editor_state.is_vertical_roll,
            );
            if let Some((iw, ih, buf)) =
                rasterize_glyph_alpha(text, grid.cols, grid.rows, tt.font_family)
            {
                let (cr, cg, cb) = (
                    (TEXT_PREVIEW_COLOR.r * 255.0) as u8,
                    (TEXT_PREVIEW_COLOR.g * 255.0) as u8,
                    (TEXT_PREVIEW_COLOR.b * 255.0) as u8,
                );
                let mut rgba = Vec::with_capacity((iw * ih * 4) as usize);
                for &a in &buf {
                    rgba.push(cr);
                    rgba.push(cg);
                    rgba.push(cb);
                    rgba.push(a);
                }
                let handle = image::Handle::from_rgba(iw, ih, rgba);
                let img = Image::new(handle).filter_method(FilterMethod::Linear);
                // 整框绘制：栅格行 0 对齐框顶，底部对齐的墨水落在框底，与音符放置区重合。
                frame.draw_image(
                    Rectangle::new(
                        Point::new(left, top),
                        Size::new((right - left).max(1.0), (bottom - top).max(1.0)),
                    ),
                    img,
                );
            }
        }
    }

    // 悬浮按钮：**仅文字工具**下显示与响应。
    //
    // 文本框几何在其它工具（含鼠标工具）下照样渲染——产物不因切换工具消失
    // （见 `Editor::pending_preview_visible` / `EditorState::set_tool`）；但
    // √×/模式 属于文字工具的输入会话（命中在 `handle_text_tool_pressed` 内），
    // 在别的工具下画出来只会是「点了没反应」的死按钮。
    if editor.current_tool() == lumino_message::Tool::Text
        && let Some(btns) = button_rects(editor)
    {
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
            font: lumino_ui_core::font::ui_font(),
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

    /// 纵向卷帘（BUG 回归）：`box_rect_screen` 必须给出转置几何。
    ///
    /// 修复前纵向下直接 `return None`，导致文本框 / √×按钮 / TextInput 覆盖层三处
    /// 同时消失——文字工具"能选中、点了没反应"。此处用**独立算术**（不调用被测函数
    /// 之外的转换封装）钉死轴向：key→X（越大越右）、tick→Y（越大越上）。
    #[test]
    fn test_box_rect_screen_vertical_transposed() {
        let mut editor = editor_with_box();
        editor.editor_state.is_vertical_roll = true;
        let (l, t, r, b) = box_rect_screen(&editor).expect("纵向卷帘必须给出文本框几何");
        let view = &editor.editor_state.view;
        let grid_bottom = editor.editor_state.canvas.size_y - view.keyboard_width;
        // key 60..64 → X = key × zoom_y − scroll_y（右缘 = key_hi 右边界 = 65 键处）
        assert_eq!(l, 60.0 * view.zoom_y - view.scroll_y, "左缘 = key_lo 的 X");
        assert_eq!(
            r,
            65.0 * view.zoom_y - view.scroll_y,
            "右缘 = key_hi 的右边界（覆盖整行 key）"
        );
        // tick 0..3840 → Y = grid_bottom − tick × zoom_x + scroll_x（tick 越大越靠上）
        assert_eq!(
            b,
            grid_bottom - 0.0 * view.zoom_x + view.scroll_x,
            "下缘 = tick_lo 的 Y"
        );
        assert_eq!(
            t,
            grid_bottom - 3840.0 * view.zoom_x + view.scroll_x,
            "上缘 = tick_hi 的 Y"
        );
        assert!(r > l, "纵向 X 为 key 轴：框必须有宽度");
        assert!(t < b, "纵向 tick 越大越靠上：上缘 Y 必须小于下缘");
    }

    /// 纵向卷帘：三个悬浮按钮（√ / × / 模式）必须存在、横向排布且完整落在卷帘内容区内。
    #[test]
    fn test_button_rects_vertical_inside_content() {
        let mut editor = editor_with_box();
        editor.editor_state.is_vertical_roll = true;
        let content = crate::grid::utils::content_bounds(&editor);
        let btns = button_rects(&editor).expect("纵向卷帘必须显示 √/×/模式 三个按钮");
        assert!(
            btns.confirm.x < btns.cancel.x && btns.cancel.x < btns.mode.x,
            "三按钮应自左向右排列"
        );
        for r in [btns.confirm, btns.cancel, btns.mode] {
            assert!(
                r.x >= content.x - 0.01 && r.y >= content.y - 0.01,
                "按钮 {r:?} 不应越出内容区左上角 {content:?}"
            );
            assert!(
                r.x + r.width <= content.x + content.width + 0.01
                    && r.y + r.height <= content.y + content.height + 0.01,
                "按钮 {r:?} 不应越出内容区右下角 {content:?}"
            );
        }
    }

    /// 「所见即生成」的**映射层**证明（纵向）：预览位图的像素轴必须与屏幕轴同向，
    /// 且与 √ 生成的音符落在同一格。
    ///
    /// 纵向：位图 x = 列 → key 递增 → 屏幕向右；位图 y = 行 → tick 递减 → 屏幕向下。
    /// 故"文字正着读"的判据是两条单调性：**列增 → 屏幕 X 增、行增 → 屏幕 Y 增**
    /// （后者等价于时间倒退，因为纵向 tick 越大越靠上）。
    #[test]
    fn test_vertical_preview_bitmap_axes_match_screen_axes() {
        use crate::interaction::text_tool::GlyphGrid;

        let mut editor = editor_with_box(); // tick [0,3840]、key [60,64]
        editor.editor_state.is_vertical_roll = true;
        let snap = editor.editor_state.view.snap_precision;
        assert_eq!(snap, 1920.0, "本测试的格数断言以默认音符精度 1920 为前提");
        let grid = GlyphGrid::from_state(&editor.editor_state.text_tool, snap, true);

        // 纵向轴角色：advance(列) = key、高度(行) = tick ⇒ 列数 = key 跨度、行数 = 时间格数
        assert_eq!(grid.cols, 5, "纵向列数 = key 跨度（60..=64）");
        assert_eq!(grid.rows, 2, "纵向行数 = tick 跨度 / snap（3840/1920）");

        let screen = |cell: (f32, u16)| editor.tick_key_to_pos(cell.0, cell.1);
        // 列 +1（advance 前进）→ 屏幕 X 增大、Y 不变（同一时间格）
        let c0 = screen(grid.cell(0, 0));
        let c1 = screen(grid.cell(0, 1));
        assert!(
            c1.x > c0.x,
            "advance 必须沿屏幕向右（正着读）：{c0:?} → {c1:?}"
        );
        assert!((c1.y - c0.y).abs() < 0.01, "同一时间格内 Y 不应变化");
        // 行 +1（字形向下）→ 屏幕 Y 增大、X 不变（同一音高格）
        let r1 = screen(grid.cell(1, 0));
        assert!(r1.y > c0.y, "字形'向下'必须沿屏幕向下：{c0:?} → {r1:?}");
        assert!((r1.x - c0.x).abs() < 0.01, "同一音高格内 X 不应变化");
    }
}
