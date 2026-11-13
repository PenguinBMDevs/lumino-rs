//! 编辑器叠加层预览（切割线 / 饼干模刀框 / 角度柔化）——egui 版。
//!
//! 取代旧版 `grid::{slice_box, mould_knife_box, round_corner_box}::draw`：
//! - 切割线：2px 橙红直线 + 两端 3px 实心圆；
//! - 饼干模：青色虚线框（每条刀音符一个）；
//! - 角度柔化：1.5px 青色路径 + 10px 橙色角点手柄（选中加白环）+
//!   拖拽提示线与 `r N.N` 半径文本。
//!
//! 全部坐标为窗口逻辑坐标（生产侧已加画布偏移）。

use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Stroke};

/// 切割线颜色。
const CUT: Color32 = Color32::from_rgba_premultiplied(230, 80, 58, 230);
/// 刀框 / 柔化路径颜色。
const KNIFE: Color32 = Color32::from_rgba_premultiplied(48, 189, 208, 242);
/// 角点手柄颜色。
const HANDLE: Color32 = Color32::from_rgb(255, 148, 31);
/// 虚线段长 / 间隔 / 线宽。
const DASH: f32 = 5.0;
const GAP: f32 = 3.0;
const WIDTH: f32 = 1.5;
/// 角点手柄边长。
const HANDLE_SIZE: f32 = 10.0;

/// 本帧叠加层预览。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EditorOverlayModel {
    /// 切割线（起点 / 终点）。
    pub slice_line: Option<(Pos2, Pos2)>,
    /// 饼干模「刀」的音符框。
    pub knife_rects: Vec<egui::Rect>,
    /// 角度柔化后的路径折线。
    pub round_path: Vec<Pos2>,
    /// 可柔化角点手柄（锚点下标 + 位置）。
    pub round_handles: Vec<(usize, Pos2)>,
    /// 拖拽提示线。
    pub round_cursor_line: Option<(Pos2, Pos2)>,
    /// 当前拖拽角点的半径。
    pub round_radius: Option<f32>,
}

impl EditorOverlayModel {
    /// 是否没有内容。
    pub fn is_empty(&self) -> bool {
        self.slice_line.is_none()
            && self.knife_rects.is_empty()
            && self.round_path.is_empty()
            && self.round_handles.is_empty()
    }
}

/// 绘制全部叠加层预览。
pub fn show(ui: &egui::Ui, model: &EditorOverlayModel) {
    let painter = ui.painter();

    if let Some((a, b)) = model.slice_line {
        painter.line_segment([a, b], Stroke::new(2.0, CUT));
        painter.circle_filled(a, 3.0, CUT);
        painter.circle_filled(b, 3.0, CUT);
    }

    for rect in &model.knife_rects {
        dashed_rect(painter, *rect);
    }

    if model.round_path.len() >= 2 {
        painter.add(egui::Shape::line(
            model.round_path.clone(),
            Stroke::new(WIDTH, KNIFE),
        ));
    }
    for (_, center) in &model.round_handles {
        let rect = egui::Rect::from_center_size(*center, egui::vec2(HANDLE_SIZE, HANDLE_SIZE));
        painter.rect_filled(rect, CornerRadius::ZERO, HANDLE);
    }
    if let Some((a, b)) = model.round_cursor_line {
        painter.line_segment([a, b], Stroke::new(1.0, KNIFE));
        if let Some(radius) = model.round_radius {
            painter.text(
                Pos2::new(b.x + 8.0, b.y - 18.0),
                Align2::LEFT_TOP,
                format!("r {radius:.1}"),
                FontId::proportional(12.0),
                KNIFE,
            );
        }
    }
}

/// 四条边都按虚线画。
fn dashed_rect(painter: &egui::Painter, rect: egui::Rect) {
    let corners = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ];
    for index in 0..4 {
        dashed_line(painter, corners[index], corners[(index + 1) % 4]);
    }
}

/// 单条虚线。
fn dashed_line(painter: &egui::Painter, from: Pos2, to: Pos2) {
    let delta = to - from;
    let len = delta.length();
    if len <= 0.5 {
        return;
    }
    let unit = delta / len;
    let mut cursor = 0.0;
    while cursor < len {
        let end = (cursor + DASH).min(len);
        painter.line_segment(
            [from + unit * cursor, from + unit * end],
            Stroke::new(WIDTH, KNIFE),
        );
        cursor = end + GAP;
    }
}
