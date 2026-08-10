//! 曲线工具直线模式渲染：锚点 + 粗连线 + √× 悬浮按钮
//!
//! 直线由两个锚点确定：
//! - 锚点：实心圆（醒目标注两端，可独立拖动）；
//! - 连线：3-4 像素粗线段（只能整体平移）；
//! - 按钮：直线右侧的 √（确认生成音符）/ ×（取消）悬浮按钮，
//!   与 i2m 区域框按钮共用同一套视觉（`confirm_buttons` 模块）。

use crate::Editor;
use crate::grid::confirm_buttons::{BUTTON_SIZE, CANCEL_ICON, CONFIRM_ICON, draw_button};
use crate::grid::utils::content_bounds;
use iced_core::{Point, Rectangle, Size};
use iced_widget::canvas::{self, Geometry, Path, Stroke};
use lumino_message::Tool;
use lumino_ui_core::Renderer;

/// 连线粗细（像素，用户要求 3-4 像素，取 4）
const LINE_WIDTH: f32 = 4.0;
/// 锚点半径（像素）
const ANCHOR_RADIUS: f32 = 6.0;
/// 锚点描边宽度（像素）
const ANCHOR_STROKE_WIDTH: f32 = 2.0;
/// 按钮组与直线中点的间距
const BUTTON_SPACING: f32 = 8.0;

/// 悬浮按钮矩形（画布坐标）
#[derive(Debug, Clone, Copy)]
pub struct LineButtonRects {
    /// √ 确认按钮
    pub confirm: Rectangle,
    /// × 取消按钮
    pub cancel: Rectangle,
}

/// 计算直线右侧悬浮按钮位置（垂直居中于直线中点）
///
/// 按钮组钳制到卷帘内容区内：直线移出/越界时按钮仍保持完整可见可点
/// （用户拖回直线后按钮自动回到其右侧）。
pub fn line_button_rects(editor: &Editor) -> Option<LineButtonRects> {
    if editor.current_tool() != Tool::Curve {
        return None;
    }
    let line = &editor.editor_state.line_tool;
    let (a, b) = (line.anchor_start?, line.anchor_end?);
    let content = content_bounds(editor);
    // 内容区高度不足以容纳单个按钮时（异常布局）不显示按钮
    if content.height < BUTTON_SIZE {
        return None;
    }
    let pa = editor.line_anchor_screen_pos(a);
    let pb = editor.line_anchor_screen_pos(b);
    let mid_x = (pa.x + pb.x) * 0.5;
    let mid_y = (pa.y + pb.y) * 0.5;

    let group_w = BUTTON_SIZE * 2.0 + BUTTON_SPACING;
    // 垂直中心钳制到内容区内，避免直线 Y 向越界时按钮悬浮到键盘/标尺上方
    let center_y = mid_y.clamp(
        content.y + BUTTON_SIZE * 0.5,
        content.y + content.height - BUTTON_SIZE * 0.5,
    );
    // 水平位置：优先直线右侧，超出内容区右边缘时钳制到右边缘
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

/// 绘制直线（锚点 + 连线）+ √× 悬浮按钮
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
    let a = line.anchor_start?;
    let b = line.anchor_end;

    let mut frame = canvas::Frame::new(renderer, bounds.size());
    let mut has_content = false;
    let anchor_color = theme.extended_palette().primary.strong.color;
    let pa = editor.line_anchor_screen_pos(a);

    // 连线（仅两锚点齐备时绘制；单锚点仅显示锚点本身）
    if let Some(b) = b {
        let pb = editor.line_anchor_screen_pos(b);
        let path = Path::new(|p| {
            p.move_to(pa);
            p.line_to(pb);
        });
        let stroke = Stroke::default()
            .with_width(LINE_WIDTH)
            .with_color(anchor_color);
        frame.stroke(&path, stroke);
        has_content = true;
    }

    // 锚点：实心圆 + 白色描边（明确标注两端锚点）
    for anchor in [Some(a), b].into_iter().flatten() {
        let pos = editor.line_anchor_screen_pos(anchor);
        let path = Path::circle(pos, ANCHOR_RADIUS);
        frame.fill(&path, anchor_color);
        let ring = Stroke::default()
            .with_width(ANCHOR_STROKE_WIDTH)
            .with_color(iced_core::Color::WHITE);
        frame.stroke(&path, ring);
        has_content = true;
    }

    // 悬浮按钮（直线完整后显示）
    if b.is_some()
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

    /// 构造曲线工具 + 完整直线的编辑器（默认视图 128 键 × 20px、画布 800x600）
    ///
    /// 锚点 key 105..110（y 364..464）与 tick 5000..5300（x 620..650）均在
    /// 可视区内，用于验证按钮垂直居中的非钳制路径。
    fn curve_editor() -> Editor {
        let mut editor = Editor::new();
        editor.editor_state.tool = Tool::Curve;
        editor.editor_state.line_tool.anchor_start = Some((5000.0, 105));
        editor.editor_state.line_tool.anchor_end = Some((5300.0, 110));
        editor.editor_state.canvas.size_x = 800.0;
        editor.editor_state.canvas.size_y = 600.0;
        editor
    }

    #[test]
    fn test_button_rects_inside_content_centered() {
        // 直线中心在内容区内时：按钮垂直居中于直线中点，且完全位于内容区内
        let editor = curve_editor();
        let btns = line_button_rects(&editor).expect("按钮应存在");
        let content = content_bounds(&editor);

        let pa = editor.line_anchor_screen_pos((5000.0, 105));
        let pb = editor.line_anchor_screen_pos((5300.0, 110));
        let mid_y = (pa.y + pb.y) * 0.5;
        let btn_center_y = btns.confirm.y + BUTTON_SIZE * 0.5;
        assert!(
            (btn_center_y - mid_y).abs() < 1.0,
            "按钮应垂直居中于直线中点（mid_y {mid_y} vs center {btn_center_y}）"
        );
        // 位于直线中点右侧
        let mid_x = (pa.x + pb.x) * 0.5;
        assert!(btns.confirm.x >= mid_x, "按钮应在直线右侧");
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
        // 非曲线工具不显示直线按钮
        let mut editor = curve_editor();
        editor.editor_state.tool = Tool::Pencil;
        assert!(line_button_rects(&editor).is_none());
    }

    #[test]
    fn test_button_rects_none_when_incomplete() {
        // 直线未完整（只有起点锚点）不显示按钮
        let mut editor = curve_editor();
        editor.editor_state.line_tool.anchor_end = None;
        assert!(line_button_rects(&editor).is_none());
    }
}
