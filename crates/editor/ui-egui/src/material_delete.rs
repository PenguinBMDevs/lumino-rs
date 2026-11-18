//! 素材删除确认（独立 OS 窗口；DRAW-045）。
//!
//! 取代主窗口内的「全屏 45% 黑遮罩 + 居中卡片」：确认属于独立决策，改为独立
//! 窗口后遮罩、吞点击与「点外部取消」全部消失，窗口 X 关闭等价于取消。
//!
//! 版式：整窗 base 底、内边距 24，内容块整体垂直居中——
//! 标题 16px / 素材名 14px（主色）/ 危险提示 13px（危险色），
//! 按钮行 34 高（危险色「删除」+ 次要「取消」、间距 12）。

use egui::{Color32, CornerRadius, FontId, Id, Rect, RichText, Stroke, Vec2};

use crate::chrome::MaterialDeleteModel;
use crate::theme;

const ACTION_ID: &str = "lumino.material-delete.action";
/// 整窗内边距。
const PAD: f32 = 24.0;
/// 标题行高（16px 文本）。
const TITLE_H: f32 = 22.0;
/// 素材名行高（14px 文本）。
const NAME_H: f32 = 19.0;
/// 危险提示行高（13px 文本）。
const WARN_H: f32 = 18.0;
/// 按钮高度。
const BUTTON_H: f32 = 34.0;
/// 按钮横向内边距（单侧）。
const BUTTON_PAD_X: f32 = 24.0;
/// 两个按钮之间的间距。
const BUTTON_GAP: f32 = 12.0;

/// 素材删除确认对话框产生的动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialDeleteAction {
    /// 取消（按钮或窗口 X）。
    Cancel,
    /// 确认删除。
    Confirm(usize),
}

/// 记录本帧动作（一帧只保留第一个）。
pub fn push_action(ctx: &egui::Context, action: MaterialDeleteAction) {
    if ctx
        .data(|d| d.get_temp::<Option<MaterialDeleteAction>>(Id::unique(ACTION_ID)))
        .flatten()
        .is_some()
    {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<MaterialDeleteAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<MaterialDeleteAction>>(Id::unique(ACTION_ID)))
        .flatten()
}

/// 渲染整个对话框窗口（`ui` = 客户端区域）。
pub fn show(ui: &mut egui::Ui, model: &MaterialDeleteModel) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());

    let rect = ui.max_rect();
    let top = if model.titlebar {
        crate::layout::TITLEBAR_H
    } else {
        0.0
    };
    let body = Rect::from_min_max(egui::pos2(rect.left(), rect.top() + top), rect.max);
    ui.painter().rect_filled(body, 0, theme::base_bg());
    let inner = body.shrink(PAD);
    if inner.width() <= 0.0 || inner.height() <= 0.0 {
        return;
    }

    // 生产覆盖层卡片：标题 → 12 → 素材名 → 8 → 危险提示 → 24 → 按钮。
    let content_h = TITLE_H + 12.0 + NAME_H + 8.0 + WARN_H + 24.0 + BUTTON_H;
    let mut y = inner.top() + ((inner.height() - content_h) * 0.5).max(0.0);
    let cx = inner.center().x;

    painter_text(ui, inner, y, TITLE_H, &model.title, 16.0, theme::text());
    y += TITLE_H + 12.0;
    painter_text(
        ui,
        inner,
        y,
        NAME_H,
        &model.name,
        14.0,
        theme::accent_strong(),
    );
    y += NAME_H + 8.0;
    painter_text(ui, inner, y, WARN_H, &model.warning, 13.0, theme::danger());
    y += WARN_H + 24.0;

    // 按钮行整体居中：取消在左、删除在右（危险色）。
    let cancel_w = button_width(ui, &model.cancel) + BUTTON_PAD_X * 2.0;
    let delete_w = button_width(ui, &model.delete) + BUTTON_PAD_X * 2.0;
    let row_w = cancel_w + BUTTON_GAP + delete_w;
    let left = cx - row_w * 0.5;

    let cancel = Rect::from_min_size(egui::pos2(left, y), Vec2::new(cancel_w, BUTTON_H));
    if button(
        ui,
        cancel,
        &model.cancel,
        theme::toolbar_bg(),
        theme::selected_bg(),
        theme::text(),
        "material-delete:cancel",
    )
    .clicked()
    {
        push_action(ui.ctx(), MaterialDeleteAction::Cancel);
    }

    let delete = Rect::from_min_size(
        egui::pos2(left + cancel_w + BUTTON_GAP, y),
        Vec2::new(delete_w, BUTTON_H),
    );
    if button(
        ui,
        delete,
        &model.delete,
        theme::danger(),
        theme::danger(),
        Color32::WHITE,
        "material-delete:confirm",
    )
    .clicked()
    {
        push_action(ui.ctx(), MaterialDeleteAction::Confirm(model.index));
    }
}

/// 水平居中的单行文本（进入可访问树）。
fn painter_text(
    ui: &mut egui::Ui,
    inner: Rect,
    y: f32,
    h: f32,
    text: &str,
    size: f32,
    color: Color32,
) {
    let rect = Rect::from_min_size(egui::pos2(inner.left(), y), Vec2::new(inner.width(), h));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(
        egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
    ));
    child.add(
        egui::Label::new(RichText::new(text).size(size).color(color))
            .selectable(false)
            .truncate(),
    );
}

/// 按钮文字宽度（与绘制同源：同一字号与字体族）。
fn button_width(ui: &egui::Ui, label: &str) -> f32 {
    ui.painter()
        .layout_no_wrap(
            label.to_owned(),
            FontId::proportional(theme::font::LABEL),
            theme::text(),
        )
        .size()
        .x
        .ceil()
}

/// 固定矩形按钮：1px 边框、圆角 4、悬停换底色。
fn button(
    ui: &mut egui::Ui,
    rect: Rect,
    label: &str,
    fill: Color32,
    hover: Color32,
    text_color: Color32,
    id: &str,
) -> egui::Response {
    let response = ui.interact(rect, Id::unique(id), egui::Sense::click());
    ui.painter().rect(
        rect,
        CornerRadius::same(4),
        if response.hovered() { hover } else { fill },
        Stroke::new(1.0, theme::border()),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        FontId::proportional(theme::font::LABEL),
        text_color,
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> MaterialDeleteModel {
        MaterialDeleteModel {
            titlebar: false,
            title: "删除素材".into(),
            name: "钢琴音色.json".into(),
            warning: "删除后无法恢复，确定要删除吗？".into(),
            cancel: "取消".into(),
            delete: "删除".into(),
            index: 3,
        }
    }

    #[test]
    fn test_model_carries_index_and_texts() {
        let m = model();
        assert_eq!(m.index, 3);
        assert_eq!(m.delete, "删除");
        assert_eq!(m.cancel, "取消");
        assert!(m.warning.contains("无法恢复"));
    }

    #[test]
    fn test_action_channel_keeps_first_action_per_frame() {
        let ctx = egui::Context::default();
        push_action(&ctx, MaterialDeleteAction::Confirm(3));
        push_action(&ctx, MaterialDeleteAction::Cancel);
        assert_eq!(
            take_action(&ctx),
            Some(MaterialDeleteAction::Confirm(3)),
            "同一帧只保留第一个动作"
        );
        assert_eq!(take_action(&ctx), None, "取走后不得残留");
    }
}
