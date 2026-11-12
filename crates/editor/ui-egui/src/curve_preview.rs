//! 曲线工具预览（路径 / 锚点 / 控制柄）——egui 版。
//!
//! 取代 iced `line_tool_box::draw` 的可见部分：
//! - 曲线段：固定 1.5px 引导线（贝塞尔；Corner 锚点退化为直线）；
//! - 描边产物：`path_outline_blocks`（与 √ 写入同源）逐块实心矩形；
//! - 锚点：半径 6 实心圆 + 白描边 2px，选中锚点外加 1 圈；
//! - 控制柄：锚点→控制柄 1px 50% 辅助线 + 8×8 方块 + 白 1px 边框。

use egui::{Color32, Pos2, Rect, Stroke};

use crate::theme;

/// 锚点半径（与 iced 一致）。
const ANCHOR_RADIUS: f32 = 6.0;
/// 锚点白描边宽度。
const ANCHOR_STROKE: f32 = 2.0;
/// 控制柄方块边长。
pub const HANDLE_SIZE: f32 = 8.0;
/// 控制柄辅助线宽度。
const HANDLE_LINE_WIDTH: f32 = 1.0;
/// 引导线宽度。
const GUIDE_WIDTH: f32 = 1.5;

/// 一段路径（窗口逻辑坐标）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CurveSegment {
    /// 直线段（Corner 弯折）。
    Line(Pos2, Pos2),
    /// 三次贝塞尔（p0, p1, p2, p3）。
    Cubic(Pos2, Pos2, Pos2, Pos2),
}

/// 锚点标记。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnchorMark {
    /// 锚点中心。
    pub pos: Pos2,
    /// 是否为当前选中锚点（加外环）。
    pub selected: bool,
}

/// 曲线预览模型（窗口逻辑坐标）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CurvePreviewModel {
    /// 曲线段。
    pub segments: Vec<CurveSegment>,
    /// 描边产物块。
    pub blocks: Vec<Rect>,
    /// 锚点 → 控制柄辅助线。
    pub connectors: Vec<(Pos2, Pos2)>,
    /// 控制柄方块。
    pub handles: Vec<Rect>,
    /// 锚点。
    pub anchors: Vec<AnchorMark>,
}

impl CurvePreviewModel {
    /// 是否没有内容。
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
            && self.blocks.is_empty()
            && self.anchors.is_empty()
            && self.handles.is_empty()
    }
}

/// 绘制曲线预览。
pub fn show(ui: &egui::Ui, model: &CurvePreviewModel) {
    let painter = ui.painter();
    let base = theme::accent_strong();
    let helper = base.gamma_multiply(0.5);
    for segment in &model.segments {
        match *segment {
            CurveSegment::Line(a, b) => {
                painter.line_segment([a, b], Stroke::new(GUIDE_WIDTH, base));
            }
            CurveSegment::Cubic(a, b, c, d) => {
                painter.add(egui::Shape::CubicBezier(
                    egui::epaint::CubicBezierShape::from_points_stroke(
                        [a, b, c, d],
                        false,
                        Color32::TRANSPARENT,
                        Stroke::new(GUIDE_WIDTH, base),
                    ),
                ));
            }
        }
    }
    for block in &model.blocks {
        if block.width() > 0.0 && block.height() > 0.0 {
            painter.rect_filled(*block, 0, base);
        }
    }
    for (a, b) in &model.connectors {
        painter.line_segment([*a, *b], Stroke::new(HANDLE_LINE_WIDTH, helper));
    }
    for handle in &model.handles {
        painter.rect_filled(*handle, 0, base);
        painter.rect_stroke(
            *handle,
            0,
            Stroke::new(1.0, Color32::WHITE),
            egui::StrokeKind::Inside,
        );
    }
    for anchor in &model.anchors {
        painter.circle_filled(anchor.pos, ANCHOR_RADIUS, base);
        painter.circle_stroke(
            anchor.pos,
            ANCHOR_RADIUS,
            Stroke::new(ANCHOR_STROKE, Color32::WHITE),
        );
        if anchor.selected {
            painter.circle_stroke(
                anchor.pos,
                ANCHOR_RADIUS + 3.0,
                Stroke::new(2.0, Color32::WHITE),
            );
        }
    }
}
