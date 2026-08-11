//! 曲线工具贝塞尔路径渲染：多路径锚点 + 曲线段 + 控制柄 + 实心填充 + 共享 √× 按钮
//!
//! 支持多条路径同时存在（批量绘制）：
//! - 锚点：实心圆（醒目标注，端点可拖动、中间锚点自由移动）；
//! - 曲线段：3-4 像素粗三次贝塞尔（控制柄与锚点重合时退化为直线）；
//! - 控制柄：方块 + 锚点间辅助线（首锚点 out、尾锚点 in、中间 in+out），
//!   拖动控制柄弯曲曲线；
//! - 实心填充：颜料桶点击封闭区域后，内部格点以半透明色块显示
//!   （√ 确认时合并生成实心音符）；
//! - 按钮：全部路径包围盒右侧的一组 √（批量确认生成音符）/ ×（批量取消）
//!   悬浮按钮，与 i2m 区域框按钮共用同一套视觉（`confirm_buttons` 模块）。

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
/// 按钮组与路径包围盒的间距
const BUTTON_SPACING: f32 = 8.0;
/// 填充色块透明度（半透明色块叠在网格上，√ 确认后成为实心音符）
const FILL_ALPHA: f32 = 0.35;

/// 悬浮按钮矩形（画布坐标）
#[derive(Debug, Clone, Copy)]
pub struct LineButtonRects {
    /// √ 确认按钮
    pub confirm: Rectangle,
    /// × 取消按钮
    pub cancel: Rectangle,
}

/// 全部路径锚点的屏幕包围盒 (min_x, max_x, min_y, max_y)
fn paths_bounds(editor: &Editor) -> Option<(f32, f32, f32, f32)> {
    let paths = &editor.editor_state.line_tool.paths;
    let mut bounds: Option<(f32, f32, f32, f32)> = None;
    for path in paths {
        for anchor in path {
            let p = editor.line_pos_screen_pos(anchor.pos);
            bounds = Some(match bounds {
                None => (p.x, p.x, p.y, p.y),
                Some((min_x, max_x, min_y, max_y)) => (
                    min_x.min(p.x),
                    max_x.max(p.x),
                    min_y.min(p.y),
                    max_y.max(p.y),
                ),
            });
        }
    }
    bounds
}

/// 计算全部路径右侧共享悬浮按钮位置（垂直居中于包围盒中心）
///
/// 按钮组钳制到卷帘内容区内：路径移出/越界时按钮仍保持完整可见可点。
pub fn line_button_rects(editor: &Editor) -> Option<LineButtonRects> {
    if editor.current_tool() != Tool::Curve {
        return None;
    }
    // 至少存在一条完整路径才显示按钮
    if !editor.editor_state.line_tool.is_complete() {
        return None;
    }
    let (min_x, max_x, min_y, max_y) = paths_bounds(editor)?;
    let content = content_bounds(editor);
    // 内容区高度不足以容纳单个按钮时（异常布局）不显示按钮
    if content.height < BUTTON_SIZE {
        return None;
    }
    let mid_x = (min_x + max_x) * 0.5;
    let mid_y = (min_y + max_y) * 0.5;

    let group_w = BUTTON_SIZE * 2.0 + BUTTON_SPACING;
    // 垂直中心钳制到内容区内，避免路径 Y 向越界时按钮悬浮到键盘/标尺上方
    let center_y = mid_y.clamp(
        content.y + BUTTON_SIZE * 0.5,
        content.y + content.height - BUTTON_SIZE * 0.5,
    );
    // 水平位置：优先包围盒右侧，超出内容区右边缘时钳制到右边缘
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

/// 绘制全部路径（锚点 + 贝塞尔段 + 控制柄）+ 共享 √× 悬浮按钮
///
/// 仅在曲线工具激活时绘制。
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
    if line.paths.is_empty() {
        return None;
    }

    let mut frame = canvas::Frame::new(renderer, bounds.size());
    let mut has_content = false;
    let anchor_color = theme.extended_palette().primary.strong.color;
    // 控制柄辅助线：主题色 50% 透明度
    let handle_line_color =
        iced_core::Color::from_rgba(anchor_color.r, anchor_color.g, anchor_color.b, 0.5);
    let white = iced_core::Color::WHITE;

    for (pi, path) in line.paths.iter().enumerate() {
        // 曲线段（贝塞尔；控制柄与锚点重合时退化为直线）
        if path.len() >= 2 {
            for pair in path.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                let pa = editor.line_pos_screen_pos(a.pos);
                let p1 = editor.line_pos_screen_pos(a.out_handle_abs());
                let p2 = editor.line_pos_screen_pos(b.in_handle_abs());
                let pb = editor.line_pos_screen_pos(b.pos);
                let curve = Path::new(|p| {
                    p.move_to(pa);
                    p.bezier_curve_to(p1, p2, pb);
                });
                let stroke = Stroke::default()
                    .with_width(LINE_WIDTH)
                    .with_color(anchor_color);
                frame.stroke(&curve, stroke);
                has_content = true;
            }
        }

        // 控制柄（辅助线 + 方块，常驻显示）
        for (ai, anchor) in path.iter().enumerate() {
            let ap = editor.line_pos_screen_pos(anchor.pos);
            for side in line.visible_handle_sides(pi, ai) {
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
        for anchor in path {
            let ap = editor.line_pos_screen_pos(anchor.pos);
            let path = Path::circle(ap, ANCHOR_RADIUS);
            frame.fill(&path, anchor_color);
            let ring = Stroke::default()
                .with_width(ANCHOR_STROKE_WIDTH)
                .with_color(white);
            frame.stroke(&path, ring);
            has_content = true;
        }
    }

    // 颜料桶实心填充：已填充格点以半透明色块显示（√ 确认后成为实心音符）
    if line.has_fill() {
        let v = &editor.editor_state.view;
        let fill_color =
            iced_core::Color::from_rgba(anchor_color.r, anchor_color.g, anchor_color.b, FILL_ALPHA);
        // 色块尺寸 = 网格单元：tick 方向一格 = snap × zoom_x（zoom_x 为 像素/tick，
        // 一格含 snap 个 tick），key 方向一行 = zoom_y
        let cell_w = (v.snap_precision.max(1.0) * v.zoom_x).max(1.0);
        let cell_h = v.zoom_y.max(1.0);
        for &(tick, key) in &line.fill {
            let p = editor.line_pos_screen_pos((tick, key as f32));
            let rect = Rectangle::new(
                Point::new(p.x - cell_w * 0.5, p.y - cell_h * 0.5),
                Size::new(cell_w, cell_h),
            );
            frame.fill(&Path::rectangle(rect.position(), rect.size()), fill_color);
        }
        has_content = true;
    }

    // 共享悬浮按钮（存在完整路径后显示）
    if let Some(btns) = line_button_rects(editor) {
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

    /// 构造曲线工具 + 两条完整路径的编辑器（默认视图 128 键 × 20px、画布 800x600）
    ///
    /// 路径 1：key 105..110（y 364..464）与 tick 5000..5300（x 620..650）；
    /// 路径 2：key 90..95（y 764..864——超出画布但几何计算不受影响）。
    fn multi_curve_editor() -> Editor {
        let mut editor = Editor::new();
        editor.editor_state.tool = Tool::Curve;
        {
            let line = &mut editor.editor_state.line_tool;
            line.paths.push(Vec::new());
            line.push_anchor(0, (5000.0, 105.0));
            line.push_anchor(0, (5300.0, 110.0));
            line.paths.push(Vec::new());
            line.push_anchor(1, (5400.0, 90.0));
            line.push_anchor(1, (5700.0, 95.0));
        }
        editor.editor_state.canvas.size_x = 800.0;
        editor.editor_state.canvas.size_y = 600.0;
        editor
    }

    #[test]
    fn test_button_rects_centered_on_all_paths() {
        // 两条路径：按钮垂直中心 = 两条路径包围盒中心
        let editor = multi_curve_editor();
        let btns = line_button_rects(&editor).expect("按钮应存在");
        let (min_x, max_x, min_y, max_y) = paths_bounds(&editor).expect("包围盒应存在");

        let mid_y = (min_y + max_y) * 0.5;
        let btn_center_y = btns.confirm.y + BUTTON_SIZE * 0.5;
        assert!(
            (btn_center_y - mid_y).abs() < 1.0,
            "按钮应垂直居中于全部路径包围盒（mid_y {mid_y} vs center {btn_center_y}）"
        );
        let mid_x = (min_x + max_x) * 0.5;
        assert!(btns.confirm.x >= mid_x, "按钮应在包围盒右侧");
        // 按钮完整位于内容区内
        let content = content_bounds(&editor);
        for rect in [btns.confirm, btns.cancel] {
            assert!(rect.x >= content.x);
            assert!(rect.y >= content.y);
            assert!(rect.x + rect.width <= content.x + content.width);
            assert!(rect.y + rect.height <= content.y + content.height);
        }
    }

    #[test]
    fn test_button_rects_none_for_other_tool() {
        let mut editor = multi_curve_editor();
        editor.editor_state.tool = Tool::Pencil;
        assert!(line_button_rects(&editor).is_none());
    }

    #[test]
    fn test_button_rects_none_without_complete_path() {
        // 只有一条未完整路径 → 不显示按钮
        let mut editor = multi_curve_editor();
        editor.editor_state.line_tool.paths[1].clear();
        editor.editor_state.line_tool.paths[0].truncate(1);
        assert!(line_button_rects(&editor).is_none());
    }
}
