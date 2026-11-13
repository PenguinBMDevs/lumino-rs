//! 曲线锚点弯折切换按钮（曲线 / 折线）——egui 版。
//!
//! 取代 旧版 `line_tool_box` 里的两枚文字按钮：52×20、圆角 4、11px 居中文本；
//! 当前弯折态用主题主色填充 + 白字 + 白边框，另一态用弱底色 + 常规文字。

use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Stroke, StrokeKind};

use crate::theme;

/// 按钮宽度（与 旧版 一致）。
pub const WIDTH: f32 = 52.0;
/// 按钮高度。
pub const HEIGHT: f32 = 20.0;

/// 弯折态（与 `lumino_editor_state::AnchorBend` 一一对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BendKind {
    /// 曲线（贝塞尔）。
    Smooth,
    /// 折线（直线）。
    Corner,
}

impl BendKind {
    /// 与生产枚举互转。
    pub fn to_anchor_bend(self) -> lumino_editor_state::AnchorBend {
        match self {
            Self::Smooth => lumino_editor_state::AnchorBend::Smooth,
            Self::Corner => lumino_editor_state::AnchorBend::Corner,
        }
    }
}

/// 单个弯折按钮。
#[derive(Debug, Clone, PartialEq)]
pub struct BendButton {
    /// 按钮矩形（窗口逻辑坐标）。
    pub rect: Rect,
    /// 文案（生产 i18n）。
    pub label: String,
    /// 该按钮代表的弯折态。
    pub kind: BendKind,
    /// 是否为当前态。
    pub selected: bool,
}

/// 弯折按钮模型。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BendButtonsModel {
    /// 两个按钮（Smooth / Corner）。
    pub buttons: Vec<BendButton>,
}

impl BendButtonsModel {
    /// 是否没有内容。
    pub fn is_empty(&self) -> bool {
        self.buttons.is_empty()
    }
}

const ACTION_ID: &str = "lumino.bend-buttons.action";

/// 记录本帧弯折动作（一帧一条）。
pub fn push_action(ctx: &egui::Context, bend: lumino_editor_state::AnchorBend) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(ACTION_ID), Some(bend)));
}

/// 取走本帧弯折动作。
pub fn take_action(ctx: &egui::Context) -> Option<lumino_editor_state::AnchorBend> {
    ctx.data_mut(|d| {
        d.remove_temp::<Option<lumino_editor_state::AnchorBend>>(egui::Id::unique(ACTION_ID))
    })
    .flatten()
}

/// 绘制两个按钮并返回本帧动作。
pub fn show(ui: &egui::Ui, model: &BendButtonsModel) -> Option<lumino_editor_state::AnchorBend> {
    let painter = ui.painter();
    let mut action = None;
    for (index, button) in model.buttons.iter().enumerate() {
        let response = ui.interact(
            button.rect,
            egui::Id::unique(("lumino.bend-button", index)),
            egui::Sense::click(),
        );
        let (fill, text, border) = if button.selected {
            (theme::accent_strong(), Color32::WHITE, Color32::WHITE)
        } else {
            (theme::toolbar_bg(), theme::text(), theme::accent_strong())
        };
        painter.rect(
            button.rect,
            CornerRadius::same(4),
            fill,
            Stroke::new(1.0, border),
            StrokeKind::Inside,
        );
        painter.text(
            Pos2::new(button.rect.center().x, button.rect.top() + 4.0),
            Align2::CENTER_TOP,
            &button.label,
            FontId::proportional(11.0),
            text,
        );
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.clicked() {
            action = Some(button.kind.to_anchor_bend());
        }
    }
    action
}
