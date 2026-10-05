//! 文字工具文本框与悬浮按钮渲染
//!
//! 激活文字工具并拉出框后，常驻绘制文本框（边框 + 淡填充）；
//! 框右侧绘制 √（确认）/ ×（取消）/ 模式切换三个悬浮按钮，
//! 视觉与曲线工具、图片转 MIDI 共用 `confirm_buttons` 模块。
//!
//! **横向 / 纵向卷帘均支持**（2026-10 补齐纵向）：`box_rect_screen` 是文本框、按钮与
//! TextInput 覆盖层的唯一几何来源，纵向走转置映射（key→X、tick→Y 且 tick 越大越靠上）；
//! 字形预览在纵向必须经 [`transpose_preview_rgba`] 转置后绘制，否则出现"预览正立、
//! 生成镜像"的分叉（生成侧见 `sample_to_notes`：行→key、列→tick）。

use crate::Editor;
use crate::grid::confirm_buttons::{BUTTON_SIZE, CANCEL_ICON, CONFIRM_ICON, draw_button};
use crate::grid::utils::content_bounds;
use crate::interaction::text_tool::rasterize_glyph_alpha;
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

/// 纵向卷帘字形预览：把字形 RGBA 位图转置为「X = key 轴、Y = tick 轴（tick 越大越靠上）」
///
/// 输入布局（与 `rasterize_glyph_alpha` 一致）：`w` = 采样列数 × SS（源 x = `col`，即 tick 方向）、
/// `h` = key 行数 × SS（源 y = `row`，行 0 = 文字顶部 = 最高 key）。
///
/// 输出布局：`w' = h`、`h' = w`，映射关系为
/// - `x' = h - 1 - row`：行 0（最高 key）→ 屏幕**最右**（纵向 key→X 同向）；
/// - `y' = w - 1 - col`：列 0（起始 tick）→ 屏幕**最下**（纵向 tick→Y 反向）。
///
/// 与音符生成侧 `interaction::text_tool::rasterize::sample_to_notes`（行→key、列→tick）
/// 严格同源，故转置后的预览与 √ 生成的音符在屏幕上**逐格重合**——纵向卷帘同样满足
/// 「看到的就是生成的」。注意这不是"把显示矩形转置一下"：转置是**镜像**变换，
/// 直接铺图会把预览画成正立，与生成的镜像图案不符。
pub(crate) fn transpose_preview_rgba(src: &[u8], w: u32, h: u32) -> Vec<u8> {
    let (dw, dh) = (h, w);
    let mut dst = vec![0u8; (dw as usize) * (dh as usize) * 4];
    for y in 0..dh {
        for x in 0..dw {
            // 逆映射：目标 (x, y) ← 源 (col, row) = (w-1-y, h-1-x)
            let sx = w - 1 - y;
            let sy = h - 1 - x;
            let si = ((sy * w + sx) * 4) as usize;
            let di = ((y * dw + x) * 4) as usize;
            dst[di..di + 4].copy_from_slice(&src[si..si + 4]);
        }
    }
    dst
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
    {
        let tt = &editor.editor_state.text_tool;
        let text = tt.text.trim();
        if !text.is_empty() {
            let snap = editor.editor_state.view.snap_precision;
            let cols = tt.cols(snap);
            let rows = tt.rows();
            if let Some((iw, ih, buf)) = rasterize_glyph_alpha(text, cols, rows, tt.font_family) {
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
                // 纵向卷帘：栅格必须**转置**后再绘制。生成侧是「行→key、列→tick」
                // （`sample_to_notes`），纵向视图又把 key 映射到 X、tick 反向映射到 Y，
                // 于是屏幕上的字形是一个**镜像变换**的结果。若按原样 `draw_image` 铺进
                // 矩形，预览会是"正立"的、与 √ 生成的镜像图案不符——即"看到一套、生成
                // 另一套"。转置后二者在同一格上逐像素重合（见 `transpose_preview_rgba`）。
                let (iw, ih, rgba) = if editor.editor_state.is_vertical_roll {
                    (ih, iw, transpose_preview_rgba(&rgba, iw, ih))
                } else {
                    (iw, ih, rgba)
                };
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

    /// 「所见即生成」的**映射层**证明：转置后的预览格与音符格同口径。
    ///
    /// 源位图 2 列 × 2 行（颜色编码 = `[col, row]`），转置后：
    /// - 左上 = (col 1, row 1) = 最高 tick × 最低 key；
    /// - 右上 = (col 1, row 0) = 最高 tick × 最高 key；
    /// - 左下 = (col 0, row 1) = 最低 tick × 最低 key；
    /// - 右下 = (col 0, row 0) = 最低 tick × 最高 key。
    ///
    /// 与纵向卷帘屏幕语义（X = key 越大越右、Y = tick 越大越上）逐项吻合。
    #[test]
    fn test_transpose_preview_maps_row_to_x_and_col_flipped_to_y() {
        let mut src = Vec::new();
        for row in 0..2u8 {
            for col in 0..2u8 {
                src.extend_from_slice(&[col, row, 0, 255]);
            }
        }
        let dst = transpose_preview_rgba(&src, 2, 2);
        assert_eq!(dst.len(), src.len(), "转置不改变总像素数（宽高互换）");
        let at = |x: usize, y: usize| {
            let i = (y * 2 + x) * 4;
            [dst[i], dst[i + 1]]
        };
        assert_eq!(at(0, 0), [1, 1], "左上 = 最高 tick × 最低 key");
        assert_eq!(at(1, 0), [1, 0], "右上 = 最高 tick × 最高 key");
        assert_eq!(at(0, 1), [0, 1], "左下 = 最低 tick × 最低 key");
        assert_eq!(at(1, 1), [0, 0], "右下 = 最低 tick × 最高 key");
    }

    /// 转置的边界：非方阵下宽高正确互换（3 列 × 2 行 → 2 × 3）。
    #[test]
    fn test_transpose_preview_non_square_swaps_dims() {
        let (w, h) = (3u32, 2u32);
        let src = vec![7u8; (w * h * 4) as usize];
        let dst = transpose_preview_rgba(&src, w, h);
        assert_eq!(dst.len(), (w * h * 4) as usize);
        assert_eq!(dst.len() / 4, (h * w) as usize, "像素数守恒（2×3 = 3×2）");
        assert!(dst.iter().all(|&b| b == 7 || b == 0), "只搬移、不合成颜色");
    }
}
