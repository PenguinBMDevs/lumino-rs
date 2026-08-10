//! 曲线工具贝塞尔路径渲染：锚点 + 曲线段 + 控制柄 + √× 悬浮按钮
//!
//! 路径由锚点链构成：
//! - 锚点：实心圆（醒目标注，端点可拖动、中间锚点自由移动）；
//! - 曲线段：3-4 像素粗三次贝塞尔（控制柄与锚点重合时退化为直线）；
//! - 控制柄：方块 + 锚点间辅助线（首锚点 out、尾锚点 in、中间 in+out），
//!   拖动控制柄弯曲曲线；
//! - 按钮：路径右侧的 √（确认生成音符）/ ×（取消）悬浮按钮，
//!   与 i2m 区域框按钮共用同一套视觉（`confirm_buttons` 模块）。

use crate::Editor;
use crate::grid::confirm_buttons::{BUTTON_SIZE, CANCEL_ICON, CONFIRM_ICON, draw_button};
use crate::grid::utils::content_bounds;
use iced_core::{Point, Rectangle, Size};
use iced_widget::canvas::{self, Geometry, Path, Stroke};
use lumino_message::Tool;
use lumino_ui_core::Renderer;

/// 曲线段粗细（像素，用户要求 3-4 像素，取 4）
const LINE_WIDTH: f32 = 4.0;
/// 锚点半径（像素）
const ANCHOR_RADIUS: f32 = 6.0;
/// 锚点描边宽度（像素）
const ANCHOR_STROKE_WIDTH: f32 = 2.0;
/// 控制柄方块边长（像素）
const HANDLE_SIZE: f32 = 8.0;
/// 控制柄辅助线宽度（像素）
const HANDLE_LINE_WIDTH: f32 = 1.0;
/// 按钮组与路径中点的间距
const BUTTON_SPACING: f32 = 8.0;

/// 悬浮按钮矩形（画布坐标）
#[derive(Debug, Clone, Copy)]
pub struct LineButtonRects {
    /// √ 确认按钮
    pub confirm: Rectangle,
    /// × 取消按钮
    pub cancel: Rectangle,
}

/// 计算路径右侧悬浮按钮位置（垂直居中于首尾锚点中点）
///
/// 按钮组钳制到卷帘内容区内：路径移出/越界时按钮仍保持完整可见可点
/// （用户拖回路径后按钮自动回到其右侧）。
pub fn line_button_rects(editor: &Editor) -> Option<LineButtonRects> {
    if editor.current_tool() != Tool::Curve {
        return None;
    }
    let anchors = &editor.editor_state.line_tool.anchors;
    // 路径未完整（少于 2 个锚点）不显示按钮
    if anchors.len() < 2 {
        return None;
    }
    let first = anchors.first()?;
    let last = anchors.last()?;
    let content = content_bounds(editor);
    // 内容区高度不足以容纳单个按钮时（异常布局）不显示按钮
    if content.height < BUTTON_SIZE {
        return None;
    }
    let pa = editor.line_pos_screen_pos(first.pos);
    let pb = editor.line_pos_screen_pos(last.pos);
    let mid_x = (pa.x + pb.x) * 0.5;
    let mid_y = (pa.y + pb.y) * 0.5;

    let group_w = BUTTON_SIZE * 2.0 + BUTTON_SPACING;
    // 垂直中心钳制到内容区内，避免路径 Y 向越界时按钮悬浮到键盘/标尺上方
    let center_y = mid_y.clamp(
        content.y + BUTTON_SIZE * 0.5,
        content.y + content.height - BUTTON_SIZE * 0.5,
    );
    // 水平位置：优先路径右侧，超出内容区右边缘时钳制到右边缘
    let x0 = (mid_x + BUTTON_SPACING).min(content.x + content.width - group_w - BUTTON_SPACING);
    // 内容区过窄无法容纳按钮组时（异常布局）不显示按钮
    if x0 < content.x + BUTTON_SPACING {
        return None;
    }
    let y0 = center_y - BUTTON_SIZE * 0.5;
    let confirm = Rectangle::new(Point::new(x0, y0), Size::new(BUTTON_SIZE, BUTTON_SIZE));
    let cancel = Rectangle::new(
        Point::new(x0 + BUTTON_SIZE + BUTTON_SPACING, y0),
        Size::new(BUTTON_SIZE, BUTTON_SIZE),
    );
    Some(LineButtonRects { confirm, cancel })
}

/// 绘制路径（锚点 + 贝塞尔段 + 控制柄）+ √× 悬浮按钮
///
/// 仅在曲线工具激活时绘制；单锚点（未完整）时仅显示锚点。
pub fn draw(
    editor: &Editor,
    renderer: &Renderer,
    theme: &lumino_ui_core::Theme,
    bounds: Rectangle,
) -> Option<Geometry<Renderer>> {
    if editor.current_tool() != Tool::Curve {
        return None;
    }
    let line = &editor.editor_state.line_tool;
    let anchors = &line.anchors;
    if anchors.is_empty() {
        return None;
    }

    let mut frame = canvas::Frame::new(renderer, bounds.size());
    let mut has_content = false;
    let anchor_color = theme.extended_palette().primary.strong.color;
    // 控制柄辅助线：主题色 50% 透明度
    let handle_line_color =
        iced_core::Color::from_rgba(anchor_color.r, anchor_color.g, anchor_color.b, 0.5);
    let white = iced_core::Color::WHITE;

    // 曲线段（贝塞尔；控制柄与锚点重合时退化为直线）
    if anchors.len() >= 2 {
        for pair in anchors.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let pa = editor.line_pos_screen_pos(a.pos);
            let p1 = editor.line_pos_screen_pos(a.out_handle_abs());
            let p2 = editor.line_pos_screen_pos(b.in_handle_abs());
            let pb = editor.line_pos_screen_pos(b.pos);
            let path = Path::new(|p| {
                p.move_to(pa);
                p.bezier_curve_to(p1, p2, pb);
            });
            let stroke = Stroke::default()
                .with_width(LINE_WIDTH)
                .with_color(anchor_color);
            frame.stroke(&path, stroke);
            has_content = true;
        }
    }

    // 控制柄（辅助线 + 方块，常驻显示）
    for (i, anchor) in anchors.iter().enumerate() {
        let ap = editor.line_pos_screen_pos(anchor.pos);
        for side in line.visible_handle_sides(i) {
            let h_abs = match side {
                lumino_editor_state::HandleSide::In => anchor.in_handle_abs(),
                lumino_editor_state::HandleSide::Out => anchor.out_handle_abs(),
            };
            let hp = editor.line_pos_screen_pos(h_abs);
            // 锚点 → 控制柄辅助线
            let aux = Path::new(|p| {
                p.move_to(ap);
                p.line_to(hp);
            });
            let aux_stroke = Stroke::default()
                .with_width(HANDLE_LINE_WIDTH)
                .with_color(handle_line_color);
            frame.stroke(&aux, aux_stroke);
            // 控制柄方块
            let rect = Rectangle::new(
                Point::new(hp.x - HANDLE_SIZE * 0.5, hp.y - HANDLE_SIZE * 0.5),
                Size::new(HANDLE_SIZE, HANDLE_SIZE),
            );
            let path = Path::rectangle(rect.position(), rect.size());
            frame.fill(&path, anchor_color);
            let ring = Stroke::default().with_width(1.0).with_color(white);
            frame.stroke(&path, ring);
            has_content = true;
        }
    }

    // 锚点：实心圆 + 白色描边（明确标注路径锚点）
    for anchor in anchors {
        let ap = editor.line_pos_screen_pos(anchor.pos);
        let path = Path::circle(ap, ANCHOR_RADIUS);
        frame.fill(&path, anchor_color);
        let ring = Stroke::default()
            .with_width(ANCHOR_STROKE_WIDTH)
            .with_color(white);
        frame.stroke(&path, ring);
        has_content = true;
    }

    // 悬浮按钮（路径完整后显示）
    if anchors.len() >= 2
        && let Some(btns) = line_button_rects(editor)
    {
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

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_core::Tool;

    /// 构造曲线工具 + 完整路径的编辑器（默认视图 128 键 × 20px、画布 800x600）
    ///
    /// 锚点 key 105..110（y 364..464）与 tick 5000..5300（x 620..650）均在
    /// 可视区内，用于验证按钮垂直居中的非钳制路径。
    fn curve_editor() -> Editor {
        let mut editor = Editor::new();
        editor.editor_state.tool = Tool::Curve;
        {
            let line = &mut editor.editor_state.line_tool;
            line.push_anchor((5000.0, 105.0));
            line.push_anchor((5300.0, 110.0));
        }
        editor.editor_state.canvas.size_x = 800.0;
        editor.editor_state.canvas.size_y = 600.0;
        editor
    }

    #[test]
    fn test_button_rects_inside_content_centered() {
        // 路径中心在内容区内时：按钮垂直居中于首尾中点，且完全位于内容区内
        let editor = curve_editor();
        let btns = line_button_rects(&editor).expect("按钮应存在");
        let content = content_bounds(&editor);

        let pa = editor.line_pos_screen_pos((5000.0, 105.0));
        let pb = editor.line_pos_screen_pos((5300.0, 110.0));
        let mid_y = (pa.y + pb.y) * 0.5;
        let btn_center_y = btns.confirm.y + BUTTON_SIZE * 0.5;
        assert!(
            (btn_center_y - mid_y).abs() < 1.0,
            "按钮应垂直居中于路径中点（mid_y {mid_y} vs center {btn_center_y}）"
        );
        // 位于路径中点右侧
        let mid_x = (pa.x + pb.x) * 0.5;
        assert!(btns.confirm.x >= mid_x, "按钮应在路径右侧");
        // 两个按钮均完整位于内容区内
        for rect in [btns.confirm, btns.cancel] {
            assert!(rect.x >= content.x);
            assert!(rect.y >= content.y);
            assert!(rect.x + rect.width <= content.x + content.width);
            assert!(rect.y + rect.height <= content.y + content.height);
        }
    }

    #[test]
    fn test_button_rects_none_for_other_tool() {
        // 非曲线工具不显示按钮
        let mut editor = curve_editor();
        editor.editor_state.tool = Tool::Pencil;
        assert!(line_button_rects(&editor).is_none());
    }

    #[test]
    fn test_button_rects_none_when_incomplete() {
        // 路径未完整（只有起点锚点）不显示按钮
        let mut editor = curve_editor();
        editor.editor_state.line_tool.anchors.truncate(1);
        assert!(line_button_rects(&editor).is_none());
    }
}
