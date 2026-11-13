//! 编辑器悬浮操作按钮（√ / ×）——egui 版。
//!
//! 取代 旧版 Canvas 里的 `confirm_buttons::draw_button`：形状工具 / 图片转 MIDI
//! 放置 / 曲线工具 / 画刷的待确认操作共用同一套 36×36 圆角按钮。
//! 几何仍由 `ui-editor` 的 `*_button_rects` 计算（避免两套布局漂移），
//! 本模块只负责绘制、命中与动作回传。

use egui::{Color32, CornerRadius, Rect, Stroke, StrokeKind};

use crate::icons::{Icon, IconCache};

/// 按钮边长（与 旧版 `BUTTON_SIZE` 一致）。
pub const BUTTON_SIZE: f32 = 36.0;
/// 圆角半径。
const RADIUS: u8 = 8;
/// 图标占按钮边长的比例（旧版 `ICON_INSET_RATIO`）。
const ICON_INSET_RATIO: f32 = 0.25;
/// 确认按钮底色（绿色）。
const CONFIRM_BG: Color32 = Color32::from_rgb(46, 125, 50);
/// 取消按钮底色（红色）。
const CANCEL_BG: Color32 = Color32::from_rgb(198, 40, 40);
/// 悬浮/按下提亮量。
const HOVER_LIGHTEN: u8 = 18;
/// 边框（白色 60%）。
const BORDER: Color32 = Color32::from_rgba_premultiplied(153, 153, 153, 153);

/// 按钮语义（生产侧翻译成 `Message`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorButtonAction {
    /// 形状工具：确认生成音符。
    ShapeConfirm,
    /// 形状工具：取消。
    ShapeCancel,
    /// 图片转 MIDI：确认放置。
    I2mConfirm,
    /// 图片转 MIDI：取消放置。
    I2mCancel,
    /// 曲线工具：确认生成音符。
    LineConfirm,
    /// 曲线工具：取消。
    LineCancel,
    /// 画刷：确认笔画。
    BrushConfirm,
    /// 画刷：取消笔画。
    BrushCancel,
}

impl EditorButtonAction {
    /// 是否为「取消」类（决定红色 / × 图标）。
    pub fn is_cancel(self) -> bool {
        matches!(
            self,
            Self::ShapeCancel | Self::I2mCancel | Self::LineCancel | Self::BrushCancel
        )
    }
}

/// 单个按钮（矩形为窗口逻辑坐标）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EditorButton {
    /// 按钮矩形。
    pub rect: Rect,
    /// 语义动作。
    pub action: EditorButtonAction,
}

/// 一帧的按钮集合。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EditorButtonsModel {
    /// 全部可见按钮（可能同时存在多组，如曲线 + 形状）。
    pub buttons: Vec<EditorButton>,
}

impl EditorButtonsModel {
    /// 是否没有按钮（零开销早退）。
    pub fn is_empty(&self) -> bool {
        self.buttons.is_empty()
    }
}

const ACTION_ID: &str = "lumino.editor-buttons.action";

/// 记录本帧按钮动作（一帧一条）。
pub fn push_action(ctx: &egui::Context, action: EditorButtonAction) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧按钮动作。
pub fn take_action(ctx: &egui::Context) -> Option<EditorButtonAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<EditorButtonAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
}

/// 绘制全部按钮并返回本帧动作。
pub fn show(
    ui: &egui::Ui,
    model: &EditorButtonsModel,
    icons: &mut IconCache,
) -> Option<EditorButtonAction> {
    let painter = ui.painter();
    let mut action = None;
    for (index, button) in model.buttons.iter().enumerate() {
        if button.rect.width() <= 0.0 || button.rect.height() <= 0.0 {
            continue;
        }
        let response = ui.interact(
            button.rect,
            egui::Id::unique(("lumino.editor-button", index)),
            egui::Sense::click(),
        );
        let cancel = button.action.is_cancel();
        let base = if cancel { CANCEL_BG } else { CONFIRM_BG };
        let fill = if response.is_pointer_button_down_on() || response.hovered() {
            lighten(base, HOVER_LIGHTEN)
        } else {
            base
        };
        painter.rect(
            button.rect,
            CornerRadius::same(RADIUS),
            fill,
            Stroke::new(1.0, BORDER),
            StrokeKind::Inside,
        );
        let inset = button.rect.width() * ICON_INSET_RATIO;
        let icon_rect = button.rect.shrink(inset);
        let icon = if cancel {
            Icon::CancelCross
        } else {
            Icon::ConfirmCheck
        };
        icons.paint_tinted(painter, icon, icon_rect, Some(Color32::WHITE));
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.clicked() {
            action = Some(button.action);
        }
    }
    action
}

/// 简单提亮（饱和加法，保持色相）。
fn lighten(color: Color32, amount: u8) -> Color32 {
    let add = |value: u8| value.saturating_add(amount);
    Color32::from_rgb(add(color.r()), add(color.g()), add(color.b()))
}
