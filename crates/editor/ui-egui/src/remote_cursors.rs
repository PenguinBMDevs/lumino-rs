//! 协作远程光标（egui 叠加层）。
//!
//! 取代旧版 `grid::remote_cursors::draw` 的可见部分：每个协作者一个 8px 圆环 +
//! 半透明填充 + 用户名标签。坐标由生产侧换算成窗口逻辑坐标，本模块只绘制。
//!
//! **绘制路径禁止每帧日志**：协作会话下每秒数百帧，任何 `tracing::debug!`
//! 都会把其它模块的日志冲掉。排查请用 puffin 作用域或断点。

use egui::{Color32, CornerRadius, FontId, Pos2, Stroke};

/// 一个远程光标。
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteCursorMark {
    /// 窗口逻辑坐标（圆心）。
    pub pos: Pos2,
    /// 用户颜色。
    pub color: Color32,
    /// 用户名。
    pub username: String,
}

/// 本帧全部远程光标。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RemoteCursorModel {
    /// 光标列表。
    pub cursors: Vec<RemoteCursorMark>,
}

impl RemoteCursorModel {
    /// 是否没有内容。
    pub fn is_empty(&self) -> bool {
        self.cursors.is_empty()
    }
}

/// 圆环半径（与旧版一致）。
const RADIUS: f32 = 8.0;
/// 标签字号。
const LABEL_SIZE: f32 = 11.0;

/// 绘制全部远程光标。
pub fn show(ui: &egui::Ui, model: &RemoteCursorModel) {
    let painter = ui.painter();
    for cursor in &model.cursors {
        let color = cursor.color;
        painter.circle_filled(cursor.pos, RADIUS, color.gamma_multiply(0.30));
        painter.circle_stroke(cursor.pos, RADIUS, Stroke::new(1.5, color));

        if cursor.username.is_empty() {
            continue;
        }
        let galley = painter.layout_no_wrap(
            cursor.username.clone(),
            FontId::proportional(LABEL_SIZE),
            Color32::WHITE,
        );
        let label = egui::Rect::from_min_size(
            Pos2::new(cursor.pos.x + RADIUS + 2.0, cursor.pos.y - RADIUS - 2.0),
            galley.size() + egui::vec2(8.0, 2.0),
        );
        painter.rect_filled(label, CornerRadius::same(3), color);
        painter.galley(label.min + egui::vec2(4.0, 1.0), galley, Color32::WHITE);
    }
}
