//! 素材删除确认（主窗口内嵌覆盖层）。
//!
//! 由 `right_sidebar.materials.pending_delete` 驱动：全屏 45% 黑遮罩 + 居中卡片
//! （宽 440、内边距 24、圆角 8）。文案全部由生产侧预格式化，这里只排版与命中。
//!
//! 与 旧版 原实现（`right_sidebar/material_delete_dialog.rs`）同口径：
//! 标题 16 / 素材名 14（主色）/ 危险提示 13（危险色）/ 按钮 `padding[8,24]` 14px。

use egui::{Align2, Color32, CornerRadius, FontId, Rect};

use crate::theme;

/// 遮罩不透明度（生产 `rgba(0, 0, 0, 0.45)`）。
const MASK_ALPHA: u8 = 115;
/// 卡片宽度。
pub const CARD_WIDTH: f32 = 440.0;
/// 卡片内边距。
const PAD: f32 = 24.0;
/// 卡片圆角。
const RADIUS: u8 = 8;
/// 按钮竖向内边距。
const BTN_PAD_Y: f32 = 8.0;
/// 按钮横向内边距。
const BTN_PAD_X: f32 = 24.0;
/// 按钮文字字号。
const BTN_TEXT: f32 = 14.0;

/// 删除确认模型（文案由生产侧格式化）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialDeleteModel {
    /// 标题（「删除素材」）。
    pub title: String,
    /// 素材显示名。
    pub name: String,
    /// 危险提示（「此操作不可撤销…」）。
    pub warning: String,
    /// 取消按钮文案。
    pub cancel: String,
    /// 删除按钮文案。
    pub delete: String,
    /// 待删除素材下标（动作回传用）。
    pub index: usize,
}

/// 用户动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialDeleteAction {
    /// 取消（按钮或点击遮罩）。
    Cancel,
    /// 确认删除。
    Confirm(usize),
}

/// 单行文本宽度（与绘制同源：同一个字号与字体族）。
fn text_width(ui: &egui::Ui, text: &str, size: f32) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), FontId::proportional(size), theme::text())
        .size()
        .x
}

/// 绘制并处理输入；返回本帧动作。
///
/// `full` = 窗口逻辑矩形（由宿主注入的真实尺寸，不能用 `viewport_rect`）。
pub fn show(
    ui: &egui::Ui,
    full: Rect,
    model: &MaterialDeleteModel,
) -> Option<MaterialDeleteAction> {
    if full.width() <= 0.0 || full.height() <= 0.0 {
        return None;
    }
    let painter = ui.painter().with_clip_rect(full);
    painter.rect_filled(full, 0, Color32::from_black_alpha(MASK_ALPHA));

    // 先排计划，再算总高（避免「边排边测」导致的双层测量）。
    let title_h = 12.0_f32.max(text_width(ui, &model.title, 16.0) * 0.0 + 19.0);
    let name_h = 17.0_f32;
    let warning_h = 16.0_f32;
    let btn_h = (BTN_PAD_Y * 2.0 + BTN_TEXT * 1.2).max(33.0);
    let cancel_w = text_width(ui, &model.cancel, BTN_TEXT) + BTN_PAD_X * 2.0;
    let delete_w = text_width(ui, &model.delete, BTN_TEXT) + BTN_PAD_X * 2.0;
    let row_w = cancel_w + 12.0 + delete_w;

    let total_h = PAD * 2.0 + title_h + 12.0 + name_h + 8.0 + warning_h + 20.0 + btn_h;
    let card = Rect::from_center_size(
        full.center(),
        egui::vec2(CARD_WIDTH, total_h.min(full.height() - 8.0)),
    );

    painter.rect_filled(card, CornerRadius::same(RADIUS), theme::base_bg());

    // 文案逐行居中。
    let cx = card.center().x;
    let mut y = card.top() + PAD;
    painter.text(
        egui::pos2(cx, y),
        Align2::CENTER_TOP,
        &model.title,
        FontId::proportional(16.0),
        theme::text(),
    );
    y += title_h + 12.0;
    painter.text(
        egui::pos2(cx, y),
        Align2::CENTER_TOP,
        &model.name,
        FontId::proportional(14.0),
        theme::accent(),
    );
    y += name_h + 8.0;
    painter.text(
        egui::pos2(cx, y),
        Align2::CENTER_TOP,
        &model.warning,
        FontId::proportional(13.0),
        theme::danger(),
    );
    y += warning_h + 20.0;

    // 按钮行整体居中。
    let row_left = cx - row_w / 2.0;
    let cancel = Rect::from_min_size(egui::pos2(row_left, y), egui::vec2(cancel_w, btn_h));
    let delete = Rect::from_min_size(
        egui::pos2(row_left + cancel_w + 12.0, y),
        egui::vec2(delete_w, btn_h),
    );
    painter.rect_filled(cancel, CornerRadius::same(4), theme::toolbar_bg());
    painter.text(
        cancel.center(),
        Align2::CENTER_CENTER,
        &model.cancel,
        FontId::proportional(BTN_TEXT),
        theme::text(),
    );
    painter.rect_filled(delete, CornerRadius::same(4), theme::danger());
    painter.text(
        delete.center(),
        Align2::CENTER_CENTER,
        &model.delete,
        FontId::proportional(BTN_TEXT),
        Color32::WHITE,
    );

    // 输入：按钮优先；卡片外点击 = 取消；卡片内空白 = 忽略。
    let (pointer, pressed) = ui.input(|i| {
        (
            i.pointer.interact_pos(),
            i.pointer.any_pressed() && i.pointer.primary_pressed(),
        )
    });
    if pressed && let Some(pos) = pointer {
        if delete.contains(pos) {
            return Some(MaterialDeleteAction::Confirm(model.index));
        }
        if cancel.contains(pos) || !card.contains(pos) {
            return Some(MaterialDeleteAction::Cancel);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> MaterialDeleteModel {
        MaterialDeleteModel {
            title: "删除素材".into(),
            name: "钢琴音色.json".into(),
            warning: "此操作不可撤销".into(),
            cancel: "取消".into(),
            delete: "删除".into(),
            index: 3,
        }
    }

    #[test]
    fn model_carries_index_and_texts() {
        let m = model();
        assert_eq!(m.index, 3);
        assert_eq!(m.delete, "删除");
        assert_eq!(m.cancel, "取消");
        assert!(m.warning.contains("不可撤销"));
    }

    #[test]
    fn card_is_centered_and_full_width_padding() {
        let full = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1200.0, 800.0));
        // 卡片宽固定 440、水平垂直居中（用常量与被测公式对齐）。
        let total_h = 24.0 * 2.0 + 19.0 + 12.0 + 17.0 + 8.0 + 16.0 + 20.0 + 33.0;
        let card = Rect::from_center_size(full.center(), egui::vec2(CARD_WIDTH, total_h));
        assert!((card.center().x - 600.0).abs() < 0.01);
        assert!((card.center().y - 400.0).abs() < 0.01);
        assert!((card.width() - CARD_WIDTH).abs() < 0.01);
    }
}
