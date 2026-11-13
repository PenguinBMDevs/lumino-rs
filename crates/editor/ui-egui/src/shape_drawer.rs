//! 形状抽屉左侧面板（`DialogType::ShapeDrawer`）。
//!
//! 库列表 + 文件 / 编辑动作 + 面积填充 + 插入与缩放 + 插入按钮。
//! 右侧画布仍由 旧版 的 `Canvas` 绘制（本层只覆盖面板区域，分割线留给 旧版 拖动）。
//!
//! 几何对齐生产 `view/shape_drawer.rs`：面板内边距 10、列间距 6、标题 16px、
//! 库行按 `spacing(2)` 排列、名称输入与动作按钮组各占一行（放不下自动换行）。

use egui::{Align, Color32, CornerRadius, FontId, RichText, Stroke, Vec2};

use crate::chrome::{ShapeActionButton, ShapeDrawerModel};
use crate::theme;

const ACTION_ID: &str = "lumino.shape-drawer.action";
/// 面板内边距（生产 `column.padding(10)`）。
const PAD: f32 = 10.0;
/// 列间距（生产 `column.spacing(6)`）。
const GAP: f32 = 6.0;
/// 按钮行内间距（生产 `row.spacing(4)`）。
const BTN_GAP: f32 = 4.0;
/// 库行间距（生产 `column(rows).spacing(2)`）。
const ROW_GAP: f32 = 2.0;
/// 输入框高度（`text_input` 默认内边距 + 16px）。
const INPUT_H: f32 = 30.0;
/// 动作按钮高度（16px 文本 + `padding([2,?])`，实测 旧版 26 逻辑）。
const BTN_H: f32 = 26.0;
/// 库行高度（16px 文本 + 内边距，实测 旧版 30 逻辑）。
const ROW_H: f32 = 30.0;

/// 面板产生的动作（统一经此通道回传）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShapeDrawerAction {
    /// 面板按钮 / 库条目动作。
    Panel(crate::chrome::ShapePanelAction),
}

/// 记录本帧动作（一帧只保留第一个）。
pub fn push_action(ctx: &egui::Context, action: ShapeDrawerAction) {
    if ctx
        .data(|d| d.get_temp::<Option<ShapeDrawerAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
        .is_some()
    {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<ShapeDrawerAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<ShapeDrawerAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
}

/// 绘制左侧面板（`rect` 由宿主按 `panel_width` 计算）。
pub fn show(ui: &mut egui::Ui, model: &ShapeDrawerModel) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());
    let ctx = ui.ctx().clone();

    let rect = model.rect;
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    // 顶部 30px 旧版 自制标题栏（红绿灯 / 拖动）仍由 旧版 绘制：egui 只画面板本体。
    let top = if model.titlebar {
        crate::layout::TITLEBAR_H
    } else {
        0.0
    };
    let body = egui::Rect::from_min_max(egui::pos2(rect.left(), rect.top() + top), rect.max);
    ui.painter().rect_filled(body, 0, theme::base_bg());
    let inner = body.shrink(PAD);
    if inner.width() <= 0.0 || inner.height() <= 0.0 {
        return;
    }

    // 自下而上算固定块高度（列间距 6 已计入）。
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
    let file_h = wrap_height(ui, inner.width(), &model.file_actions);
    let edit_h = wrap_height(ui, inner.width(), &model.edit_actions);
    let bottom = 24.0      // 标题
        + GAP
        + INPUT_H + GAP    // 名称输入
        + file_h + GAP     // 文件动作（可折行）
        + edit_h + GAP     // 编辑动作（可折行）
        + BTN_H + GAP      // 面积开关行
        + swatch_height(inner.width(), model.swatches.len()) + GAP // 色块（按宽度折行）
        + 14.0 + GAP       // 面积提示
        + status_h
        + warning_h
        + BTN_H + GAP      // Size 行
        + BTN_H + GAP      // Scale 行
        + BTN_H + GAP      // 弯折（或空占位）
        + 32.0; // Insert
    let list_h = (inner.height() - bottom).max(ROW_H * 2.0);

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

    // 名称输入。
    input(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(inner.left(), y),
            Vec2::new(inner.width(), INPUT_H),
        ),
        "shape-name",
        &model.name_input,
        &model.name_hint,
        |text| act(&ctx, crate::chrome::ShapePanelAction::SetName(text)),
    );
    y += INPUT_H + GAP;

    // 文件动作 / 编辑动作（自动换行）。
    y += buttons_wrapped(ui, inner, y, &model.file_actions);
    y += buttons_wrapped(ui, inner, y, &model.edit_actions);

    // 面积开关行：开关（填满）+ Clear。
    let clear_w = button_width(ui, &model.area_clear, 16.0);
    let toggle_w = (inner.width() - clear_w - BTN_GAP).max(60.0);
    // 旧版 默认 `button`：所有面板按钮都是主色底（面积开关也是）。
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
        act(&ctx, crate::chrome::ShapePanelAction::ToggleAreaMode);
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
        act(&ctx, crate::chrome::ShapePanelAction::ClearAreas);
    }
    y += BTN_H + GAP;

    // 色块（自动换行）。
    y += swatches(ui, inner, y, model);

    // 面积提示。
    label_left(
        ui,
        egui::Rect::from_min_size(egui::pos2(inner.left(), y), Vec2::new(inner.width(), 14.0)),
        &model.area_hint,
        10.0,
        theme::text(),
    );
    y += 14.0 + GAP;

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

    // Size 行：标签 + 42 宽输入 + 「x」+ 42 宽输入 + Contain/Stretch。
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
        |text| act(&ctx, crate::chrome::ShapePanelAction::SetInsertBeats(text)),
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
        |text| act(&ctx, crate::chrome::ShapePanelAction::SetInsertKeys(text)),
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
        act(&ctx, crate::chrome::ShapePanelAction::ToggleInsertFit);
    }
    y += BTN_H + GAP;

    // Scale 行：标签 + 46 宽输入 + Apply。
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
    x = inner.left() + scale_label_w + 4.0;
    input(
        ui,
        egui::Rect::from_min_size(egui::pos2(x, y), Vec2::new(46.0, BTN_H)),
        "shape-scale",
        &model.scale_percent,
        &model.scale_hint,
        |text| act(&ctx, crate::chrome::ShapePanelAction::SetScalePercent(text)),
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
        act(&ctx, crate::chrome::ShapePanelAction::ApplyScale);
    }
    y += BTN_H + GAP;

    // 锚点弯折（或空占位）。
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
            act(&ctx, crate::chrome::ShapePanelAction::ToggleAnchorBend);
        }
    }
    y += BTN_H + GAP;

    // Insert（填满）。
    if button(
        ui,
        egui::Rect::from_min_size(egui::pos2(inner.left(), y), Vec2::new(inner.width(), 32.0)),
        &model.insert,
        theme::accent(),
        theme::on_accent(),
    )
    .clicked()
    {
        act(&ctx, crate::chrome::ShapePanelAction::Insert);
    }
}

/// 库列表（可滚动，行距 2）。
fn library(ui: &mut egui::Ui, rect: egui::Rect, model: &ShapeDrawerModel) {
    let ctx = ui.ctx().clone();
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(Align::Min)),
    );
    {
        let scroll = &mut child.spacing_mut().scroll;
        scroll.bar_width = 8.0;
        scroll.bar_inner_margin = 0.0;
        scroll.bar_outer_margin = 0.0;
        scroll.floating = false;
    }
    let visuals = child.visuals_mut();
    visuals.widgets.inactive.fg_stroke.color = theme::selected_bg();
    visuals.widgets.hovered.fg_stroke.color = theme::hover_border();
    visuals.widgets.active.fg_stroke.color = theme::active_bg();

    egui::ScrollArea::vertical()
        .id_salt("lumino.shape-drawer.library")
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            ui.spacing_mut().item_spacing.y = ROW_GAP;
            let width = ui.available_width().max(1.0);
            for row in &model.rows {
                let (rect, response) =
                    ui.allocate_exact_size(Vec2::new(width, ROW_H), egui::Sense::click());
                response.widget_info(|| {
                    egui::WidgetInfo::selected(egui::Role::Button, true, row.selected, &row.label)
                });
                // 旧版 默认 `button`：库行整行主色底 + 白字（选中态靠「● 」前缀）。
                let fill = if response.hovered() {
                    theme::accent_strong()
                } else {
                    theme::accent()
                };
                ui.painter().rect_filled(rect, CornerRadius::same(3), fill);
                label_left_in(
                    ui,
                    egui::Rect::from_min_max(
                        egui::pos2(rect.left() + 4.0, rect.top()),
                        egui::pos2(rect.right(), rect.bottom()),
                    ),
                    &row.label,
                    16.0,
                    theme::on_accent(),
                );
                if response.clicked() {
                    act(
                        &ctx,
                        crate::chrome::ShapePanelAction::Select(row.id.clone()),
                    );
                }
            }
        });
}

/// 色块单元宽（16px 文本 + 内边距，与 旧版 观感一致）。
const SWATCH_W: f32 = 31.0;
/// 色块高。
const SWATCH_H: f32 = 22.0;
/// 色块间距。
const SWATCH_GAP: f32 = 2.0;

/// 每行能放下的色块数。
fn swatch_per_row(width: f32) -> usize {
    (((width + SWATCH_GAP) / (SWATCH_W + SWATCH_GAP)).floor() as usize).max(1)
}

/// 色块总行数。
fn swatch_rows(width: f32, count: usize) -> usize {
    let per = swatch_per_row(width);
    count.div_ceil(per)
}

/// 色块区总高（不含列间距）。
fn swatch_height(width: f32, count: usize) -> f32 {
    let rows = swatch_rows(width, count) as f32;
    rows * SWATCH_H + (rows - 1.0) * SWATCH_GAP
}

/// 折行后的行数（与 `buttons_wrapped` 同一套算法）。
fn wrap_rows(ui: &egui::Ui, width: f32, buttons: &[ShapeActionButton]) -> usize {
    let left = 0.0;
    let right = width;
    let mut x = left;
    let mut rows = 1usize;
    for button_def in buttons {
        let w = button_width(ui, &button_def.label, 16.0);
        if x + w > right && x > left {
            x = left;
            rows += 1;
        }
        x += w + BTN_GAP;
    }
    rows
}

/// 折行总高（不含列间距）。
fn wrap_height(ui: &egui::Ui, width: f32, buttons: &[ShapeActionButton]) -> f32 {
    let rows = wrap_rows(ui, width, buttons) as f32;
    rows * BTN_H + (rows - 1.0) * BTN_GAP
}

/// 一行（可折行）动作按钮，返回总高。
fn buttons_wrapped(
    ui: &mut egui::Ui,
    inner: egui::Rect,
    y: f32,
    buttons: &[ShapeActionButton],
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
        // 旧版 里 16 个色块都是默认按钮（主色底 + 白字），选中通过面积色体现。
        let (fill, text) = (theme::accent(), theme::on_accent());
        ui.painter().rect(
            rect,
            CornerRadius::same(3),
            fill,
            Stroke::new(1.0, theme::selected_bg()),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            &swatch.label,
            FontId::proportional(14.0),
            text,
        );
        if response.clicked() {
            act(
                ui,
                crate::chrome::ShapePanelAction::SetAreaColour(swatch.colour),
            );
        }
        x += SWATCH_W + SWATCH_GAP;
    }
    swatch_height(inner.width(), model.swatches.len())
}

/// 面板按钮（`padding([2,6])` 口径）。
fn button(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    label: &str,
    fill: Color32,
    text: Color32,
) -> egui::Response {
    let button = egui::Button::new(RichText::new(label).size(16.0).color(text))
        .fill(fill)
        .corner_radius(CornerRadius::same(3));
    ui.put(rect, button)
}

/// 固定盒子输入框。
fn input(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    id: &'static str,
    value: &str,
    placeholder: &str,
    changed: impl FnOnce(String),
) {
    let mut buffer = value.to_owned();
    ui.painter().rect(
        rect,
        CornerRadius::same(4),
        theme::base_bg(),
        Stroke::new(1.0, theme::selected_bg()),
        egui::StrokeKind::Inside,
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(6.0, 5.0)))
            .layout(egui::Layout::left_to_right(Align::Center)),
    );
    let response = child
        .push_id(("shape-input", id), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut buffer)
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::ZERO)
                    .font(FontId::proportional(14.0))
                    .text_color(theme::text())
                    .hint_text(
                        RichText::new(placeholder)
                            .size(14.0)
                            .color(theme::text_dim()),
                    )
                    .desired_width((rect.width() - 14.0).max(10.0)),
            )
        })
        .inner;
    if response.changed() {
        changed(buffer);
    }
}

/// 记录面板动作。
fn act(ctx: &egui::Context, action: crate::chrome::ShapePanelAction) {
    push_action(ctx, ShapeDrawerAction::Panel(action));
}

/// 按钮宽度 = 文本宽 + 左右内边距（`style.spacing.button_padding`）+ 2 边框余量。
///
/// 只用文本宽会让 egui 的按钮内边距挤占文字，出现「Ne / Sav / Renam」这类折行。
fn button_width(ui: &egui::Ui, text: &str, size: f32) -> f32 {
    let pad = ui.spacing().button_padding.x * 2.0 + 2.0;
    text_width(ui, text, size) + pad
}

fn text_width(ui: &egui::Ui, text: &str, size: f32) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), FontId::proportional(size), theme::text())
        .size()
        .x
        .ceil()
}

/// 固定矩形内的左对齐文本（保留可访问树节点）。
fn label_left(ui: &mut egui::Ui, rect: egui::Rect, text: &str, size: f32, color: Color32) {
    label_left_in(ui, rect, text, size, color);
}

fn label_left_in(ui: &mut egui::Ui, rect: egui::Rect, text: &str, size: f32, color: Color32) {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(Align::Center)),
    );
    child.add(egui::Label::new(RichText::new(text).size(size).color(color)).selectable(false));
}
