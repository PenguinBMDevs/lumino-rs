//! 形状抽屉面板：标题 + 库列表 + 名称/动作 + 面积 + 填充/门长 + 开关/状态 +
//! 尺寸/缩放 + 弯折/插入。
//!
//! 每个区块返回新的纵向游标，`show` 只负责次序；区块高度与绘制共用
//! `layout` 里的折行 / 色块测量，保证「算出来」与「画出来」一致。

use crate::chrome::{ShapeDrawerModel, ShapePanelAction};
use crate::text::text_width_ceil as text_width;
use crate::theme;
use egui::{Align, RichText, Vec2};

use super::layout::{
    BTN_GAP, BTN_H, GAP, INPUT_H, PAD, ROW_H, SWATCH_GAP, SWATCH_W, button_width, swatch_height,
    wrap_height,
};
use super::library::library;
use super::widgets::{act, button, input, label_left};

/// 绘制左侧面板（`rect` 由宿主按 `panel_width` 计算）。
pub fn show(ui: &mut egui::Ui, model: &ShapeDrawerModel) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());

    let rect = model.rect;
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    // 顶部 30px 自制标题栏（红绿灯 / 拖动）由 egui 绘制（`chrome::popup_titlebar`），
    // 本模块只画面板本体。
    let body = crate::layout::body_rect(rect, model.titlebar);
    ui.painter().rect_filled(body, 0, theme::base_bg());
    let inner = body.shrink(PAD);
    if inner.width() <= 0.0 || inner.height() <= 0.0 {
        return;
    }

    let list_h = (inner.height() - bottom_height(ui, inner.width(), model)).max(ROW_H * 2.0);

    let mut y = inner.top();
    label_left(
        ui,
        egui::Rect::from_min_size(egui::pos2(inner.left(), y), Vec2::new(inner.width(), 24.0)),
        &model.title,
        16.0,
        theme::text(),
    );
    y += 24.0 + GAP;

    // 库列表。
    let list_rect = egui::Rect::from_min_size(
        egui::pos2(inner.left(), y),
        Vec2::new(inner.width(), list_h),
    );
    library(ui, list_rect, model);
    y += list_h + GAP;

    y = name_row(ui, inner, y, model);
    y = action_rows(ui, inner, y, model);
    y = area_toggle_row(ui, inner, y, model);
    y = area_hints(ui, inner, y, model);
    y = fill_row(ui, inner, y, model);
    y = gate_row(ui, inner, y, model);
    y = switch_row(ui, inner, y, model);
    y = status_and_warning(ui, inner, y, model);
    y = size_row(ui, inner, y, model);
    y = scale_row(ui, inner, y, model);
    y = bend_row(ui, inner, y, model);
    insert_button(ui, inner, y, model);
}

/// 自下而上算固定块高度（列间距 6 已计入）。
fn bottom_height(ui: &egui::Ui, width: f32, model: &ShapeDrawerModel) -> f32 {
    let status_h = if model.status.is_some() {
        16.0 + GAP
    } else {
        0.0
    };
    let warning_h = if model.warning.is_some() {
        26.0 + GAP
    } else {
        0.0
    };
    let file_h = wrap_height(ui, width, &model.file_actions);
    let edit_h = wrap_height(ui, width, &model.edit_actions);
    let fill_h = wrap_height(ui, width, &model.fill_actions);
    let switch_h = wrap_height(ui, width, &model.switch_actions);
    24.0      // 标题
        + GAP
        + INPUT_H + GAP    // 名称输入
        + file_h + GAP     // 文件动作（可折行）
        + edit_h + GAP     // 编辑动作（可折行）
        + BTN_H + GAP      // 面积开关行
        + swatch_height(width, model.swatches.len()) + GAP // 色块（按宽度折行）
        + 14.0 + GAP       // 面积提示
        + 14.0 + GAP       // 通道 10 提示
        + fill_h + GAP     // 填充档位
        + BTN_H + GAP      // 门长
        + switch_h + GAP   // 三个开关
        + status_h
        + warning_h
        + BTN_H + GAP      // Size 行
        + BTN_H + GAP      // Scale 行
        + BTN_H + GAP      // 弯折（或空占位）
        + 32.0 // Insert
}

/// 名称输入。
fn name_row(ui: &mut egui::Ui, inner: egui::Rect, y: f32, model: &ShapeDrawerModel) -> f32 {
    let ctx = ui.ctx().clone();
    input(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(inner.left(), y),
            Vec2::new(inner.width(), INPUT_H),
        ),
        "shape-name",
        &model.name_input,
        &model.name_hint,
        |text| act(&ctx, ShapePanelAction::SetName(text)),
    );
    y + INPUT_H + GAP
}

/// 文件动作 / 编辑动作（自动换行）。
fn action_rows(ui: &mut egui::Ui, inner: egui::Rect, y: f32, model: &ShapeDrawerModel) -> f32 {
    let y = y + buttons_wrapped(ui, inner, y, &model.file_actions);
    y + buttons_wrapped(ui, inner, y, &model.edit_actions)
}

/// 面积开关行：开关（填满）+ Clear。
fn area_toggle_row(ui: &mut egui::Ui, inner: egui::Rect, y: f32, model: &ShapeDrawerModel) -> f32 {
    let ctx = ui.ctx().clone();
    let clear_w = button_width(ui, &model.area_clear, 16.0);
    let toggle_w = (inner.width() - clear_w - BTN_GAP).max(60.0);
    // 上一代 默认 `button`：所有面板按钮都是主色底（面积开关也是）。
    let (fill, text) = (theme::accent(), theme::on_accent());
    if button(
        ui,
        egui::Rect::from_min_size(egui::pos2(inner.left(), y), Vec2::new(toggle_w, BTN_H)),
        &model.area_toggle_label,
        fill,
        text,
    )
    .clicked()
    {
        act(&ctx, ShapePanelAction::ToggleAreaMode);
    }
    if button(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(inner.right() - clear_w, y),
            Vec2::new(clear_w, BTN_H),
        ),
        &model.area_clear,
        theme::accent(),
        theme::on_accent(),
    )
    .clicked()
    {
        act(&ctx, ShapePanelAction::ClearAreas);
    }
    y + BTN_H + GAP
}

/// 色块（自动换行）+ 面积提示 + 通道 10 提示。
fn area_hints(ui: &mut egui::Ui, inner: egui::Rect, y: f32, model: &ShapeDrawerModel) -> f32 {
    let mut y = y + swatches(ui, inner, y, model);

    // 面积提示。
    label_left(
        ui,
        egui::Rect::from_min_size(egui::pos2(inner.left(), y), Vec2::new(inner.width(), 14.0)),
        &model.area_hint,
        10.0,
        theme::text(),
    );
    y += 14.0 + GAP;

    // 通道 10 提示（DRAW-032）。
    label_left(
        ui,
        egui::Rect::from_min_size(egui::pos2(inner.left(), y), Vec2::new(inner.width(), 14.0)),
        &model.use10_hint,
        10.0,
        theme::text_muted(),
    );
    y + 14.0 + GAP
}

/// 填充档位（DRAW-011）：Fill / Align / Ends 循环按钮。
fn fill_row(ui: &mut egui::Ui, inner: egui::Rect, y: f32, model: &ShapeDrawerModel) -> f32 {
    y + buttons_wrapped(ui, inner, y, &model.fill_actions)
}

/// Spam 门长：Gate - 值 +。
fn gate_row(ui: &mut egui::Ui, inner: egui::Rect, y: f32, model: &ShapeDrawerModel) -> f32 {
    let ctx = ui.ctx().clone();
    let gate_label_w = text_width(ui, &model.gate_label, 11.0);
    label_left(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(inner.left(), y),
            Vec2::new(gate_label_w + 4.0, BTN_H),
        ),
        &model.gate_label,
        11.0,
        theme::text(),
    );
    let mut gx = inner.left() + gate_label_w + 4.0;
    let step_w = 26.0;
    if button(
        ui,
        egui::Rect::from_min_size(egui::pos2(gx, y), Vec2::new(step_w, BTN_H)),
        &model.gate_minus,
        theme::accent(),
        theme::on_accent(),
    )
    .clicked()
    {
        act(
            &ctx,
            ShapePanelAction::SetGateBeats((model.gate_beats - 0.0625).max(0.001)),
        );
    }
    gx += step_w + BTN_GAP;
    let value = format!("{:.3}", model.gate_beats);
    let value_w = text_width(ui, &value, 11.0);
    label_left(
        ui,
        egui::Rect::from_min_size(egui::pos2(gx, y), Vec2::new(value_w + 4.0, BTN_H)),
        &value,
        11.0,
        theme::text(),
    );
    gx += value_w + 4.0 + BTN_GAP;
    if button(
        ui,
        egui::Rect::from_min_size(egui::pos2(gx, y), Vec2::new(step_w, BTN_H)),
        &model.gate_plus,
        theme::accent(),
        theme::on_accent(),
    )
    .clicked()
    {
        act(
            &ctx,
            ShapePanelAction::SetGateBeats(model.gate_beats + 0.0625),
        );
    }
    y + BTN_H + GAP
}

/// Union / Apart / Borders 三开关。
fn switch_row(ui: &mut egui::Ui, inner: egui::Rect, y: f32, model: &ShapeDrawerModel) -> f32 {
    y + buttons_wrapped(ui, inner, y, &model.switch_actions)
}

/// 状态行 / 折行警告（存在才占位）。
fn status_and_warning(
    ui: &mut egui::Ui,
    inner: egui::Rect,
    y: f32,
    model: &ShapeDrawerModel,
) -> f32 {
    let mut y = y;
    if let Some(status) = &model.status {
        label_left(
            ui,
            egui::Rect::from_min_size(egui::pos2(inner.left(), y), Vec2::new(inner.width(), 16.0)),
            status,
            11.0,
            theme::text(),
        );
        y += 16.0 + GAP;
    }
    if let Some(warning) = &model.warning {
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(egui::Rect::from_min_size(
                    egui::pos2(inner.left(), y),
                    Vec2::new(inner.width(), 26.0),
                ))
                .layout(egui::Layout::top_down(Align::Min)),
        );
        child.add(egui::Label::new(RichText::new(warning).size(10.0).color(theme::text())).wrap());
        y += 26.0 + GAP;
    }
    y
}

/// Size 行：标签 + 42 宽输入 + 「x」+ 42 宽输入 + Contain/Stretch。
fn size_row(ui: &mut egui::Ui, inner: egui::Rect, y: f32, model: &ShapeDrawerModel) -> f32 {
    let ctx = ui.ctx().clone();
    let label_w = text_width(ui, &model.size_label, 11.0);
    label_left(
        ui,
        egui::Rect::from_min_size(egui::pos2(inner.left(), y), Vec2::new(label_w + 4.0, BTN_H)),
        &model.size_label,
        11.0,
        theme::text(),
    );
    let mut x = inner.left() + label_w + 4.0;
    input(
        ui,
        egui::Rect::from_min_size(egui::pos2(x, y), Vec2::new(42.0, BTN_H)),
        "shape-beats",
        &model.insert_beats,
        &model.insert_beats_hint,
        |text| act(&ctx, ShapePanelAction::SetInsertBeats(text)),
    );
    x += 42.0 + BTN_GAP;
    let times_w = text_width(ui, &model.times_label, 11.0);
    label_left(
        ui,
        egui::Rect::from_min_size(egui::pos2(x, y), Vec2::new(times_w + 4.0, BTN_H)),
        &model.times_label,
        11.0,
        theme::text(),
    );
    x += times_w + 4.0;
    input(
        ui,
        egui::Rect::from_min_size(egui::pos2(x, y), Vec2::new(42.0, BTN_H)),
        "shape-keys",
        &model.insert_keys,
        &model.insert_keys_hint,
        |text| act(&ctx, ShapePanelAction::SetInsertKeys(text)),
    );
    x += 42.0 + BTN_GAP;
    let fit_w = button_width(ui, &model.fit_label, 16.0);
    if button(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(x, y),
            Vec2::new(fit_w.min(inner.right() - x), BTN_H),
        ),
        &model.fit_label,
        theme::accent(),
        theme::on_accent(),
    )
    .clicked()
    {
        act(&ctx, ShapePanelAction::ToggleInsertFit);
    }
    y + BTN_H + GAP
}

/// Scale 行：标签 + 46 宽输入 + Apply。
fn scale_row(ui: &mut egui::Ui, inner: egui::Rect, y: f32, model: &ShapeDrawerModel) -> f32 {
    let ctx = ui.ctx().clone();
    let scale_label_w = text_width(ui, &model.scale_label, 11.0);
    label_left(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(inner.left(), y),
            Vec2::new(scale_label_w + 4.0, BTN_H),
        ),
        &model.scale_label,
        11.0,
        theme::text(),
    );
    let mut x = inner.left() + scale_label_w + 4.0;
    input(
        ui,
        egui::Rect::from_min_size(egui::pos2(x, y), Vec2::new(46.0, BTN_H)),
        "shape-scale",
        &model.scale_percent,
        &model.scale_hint,
        |text| act(&ctx, ShapePanelAction::SetScalePercent(text)),
    );
    x += 46.0 + BTN_GAP;
    let apply_w = button_width(ui, &model.scale_apply, 16.0);
    if button(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(x, y),
            Vec2::new(apply_w.min(inner.right() - x), BTN_H),
        ),
        &model.scale_apply,
        theme::accent(),
        theme::on_accent(),
    )
    .clicked()
    {
        act(&ctx, ShapePanelAction::ApplyScale);
    }
    y + BTN_H + GAP
}

/// 锚点弯折（或空占位）。
fn bend_row(ui: &mut egui::Ui, inner: egui::Rect, y: f32, model: &ShapeDrawerModel) -> f32 {
    let ctx = ui.ctx().clone();
    if let Some(bend) = &model.bend {
        let w = button_width(ui, bend, 16.0);
        if button(
            ui,
            egui::Rect::from_min_size(egui::pos2(inner.left(), y), Vec2::new(w, BTN_H)),
            bend,
            theme::accent(),
            theme::on_accent(),
        )
        .clicked()
        {
            act(&ctx, ShapePanelAction::ToggleAnchorBend);
        }
    }
    y + BTN_H + GAP
}

/// Insert（填满）。
fn insert_button(ui: &mut egui::Ui, inner: egui::Rect, y: f32, model: &ShapeDrawerModel) {
    let ctx = ui.ctx().clone();
    if button(
        ui,
        egui::Rect::from_min_size(egui::pos2(inner.left(), y), Vec2::new(inner.width(), 32.0)),
        &model.insert,
        theme::accent(),
        theme::on_accent(),
    )
    .clicked()
    {
        act(&ctx, ShapePanelAction::Insert);
    }
}

/// 一行（可折行）动作按钮，返回总高。
fn buttons_wrapped(
    ui: &mut egui::Ui,
    inner: egui::Rect,
    y: f32,
    buttons: &[crate::chrome::ShapeActionButton],
) -> f32 {
    let ctx = ui.ctx().clone();
    let mut x = inner.left();
    let mut row = 0.0;
    for button_def in buttons {
        let w = button_width(ui, &button_def.label, 16.0);
        if x + w > inner.right() && x > inner.left() {
            x = inner.left();
            row += 1.0;
        }
        let rect = egui::Rect::from_min_size(
            egui::pos2(x, y + row * (BTN_H + BTN_GAP)),
            Vec2::new(w, BTN_H),
        );
        if button(
            ui,
            rect,
            &button_def.label,
            theme::accent(),
            theme::on_accent(),
        )
        .clicked()
        {
            act(&ctx, button_def.action.clone());
        }
        x += w + BTN_GAP;
    }
    wrap_height(ui, inner.width(), buttons)
}

/// 色块行（自动换行），返回总高。
fn swatches(ui: &mut egui::Ui, inner: egui::Rect, y: f32, model: &ShapeDrawerModel) -> f32 {
    let mut x = inner.left();
    let mut row = 0.0;
    let w = 31.0;
    for swatch in &model.swatches {
        if x + w > inner.right() && x > inner.left() {
            x = inner.left();
            row += 1.0;
        }
        let rect =
            egui::Rect::from_min_size(egui::pos2(x, y + row * (22.0 + 2.0)), Vec2::new(w, 22.0));
        let response = ui.interact(
            rect,
            egui::Id::unique(("shape-swatch", swatch.colour)),
            egui::Sense::click(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::Button, true, swatch.selected, &swatch.label)
        });
        // 上一代 里 16 个色块都是默认按钮（主色底 + 白字），选中通过面积色体现。
        let (fill, text) = (theme::accent(), theme::on_accent());
        ui.painter().rect(
            rect,
            egui::CornerRadius::same(3),
            fill,
            egui::Stroke::new(1.0, theme::selected_bg()),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            &swatch.label,
            egui::FontId::proportional(14.0),
            text,
        );
        if response.clicked() {
            act(ui.ctx(), ShapePanelAction::SetAreaColour(swatch.colour));
        }
        x += SWATCH_W + SWATCH_GAP;
    }
    swatch_height(inner.width(), model.swatches.len())
}
