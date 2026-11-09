//! 画刷「绘制行为」对话框（`DialogType::BrushSettings`）。
//!
//! 内边距 16、列间距 8：标题 18px；可滚动的逐层音轨分配列表。
//!
//! 每行 = 64 宽层名 + 8 + 200 宽音轨下拉 + 8 + 22×22「+」+ 4 + 22×22「−」，行距 8；
//! 底部「保存」「取消」（`padding([6,16])`）。
//!
//! 无可用音轨时列表整体替换为一行禁用提示（与生产一致）。
//! 本对话框只编辑「每层分配到的音轨」，层数 = 全局粗细，由粗细面板负责。

use egui::{Align, Color32, CornerRadius, FontId, RichText, Sense, Stroke, Vec2};

use crate::chrome::BrushSettingsModel;
use crate::theme;

const ACTION_ID: &str = "lumino.brush-settings.action";
/// 内边距（生产 `column.padding(16)`）。
const PAD: f32 = 16.0;
/// 层名宽度（生产 `text.width(Fixed(64))`）。
const LABEL_W: f32 = 64.0;
/// 音轨下拉宽度（生产 `pick_list.width(Fixed(200))`）。
const COMBO_W: f32 = 200.0;
/// 行内控件高度（pick_list 与图标按钮同高 22）。
const ROW_H: f32 = 22.0;
/// 层行间距（生产 `column(rows).spacing(8)`）。
const ROW_GAP: f32 = 8.0;
/// 图标按钮（18×18 图标 + `padding(2)`）。
const ICON_BTN: f32 = 22.0;
/// 底部按钮（生产 `padding([6,16])` + 14px）。
const BUTTON_H: f32 = 30.0;
const BUTTON_GAP: f32 = 8.0;

/// 画刷设置对话框产生的动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrushSettingsAction {
    /// 第 `level` 层改绑音轨（`None` = 默认自动分配）。
    LevelTrackChanged(usize, Option<usize>),
    /// 在第 `level` 层下方插入新层。
    AddLevel(usize),
    /// 删除第 `level` 层。
    RemoveLevel(usize),
    /// 保存（把草稿写回生产）。
    Save,
    /// 取消。
    Cancel,
}

/// 记录本帧动作（一帧只保留第一个）。
pub fn push_action(ctx: &egui::Context, action: BrushSettingsAction) {
    if ctx
        .data(|d| d.get_temp::<Option<BrushSettingsAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
        .is_some()
    {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<BrushSettingsAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<BrushSettingsAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
}

/// 渲染整个对话框窗口（`ui` = 客户端区域）。
pub fn show(ui: &mut egui::Ui, model: &BrushSettingsModel) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());

    let rect = ui.max_rect();
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

    let mut y = inner.top();
    label_block(
        ui,
        egui::Rect::from_min_size(egui::pos2(inner.left(), y), Vec2::new(inner.width(), 27.0)),
        &model.title,
        18.0,
        theme::text(),
    );
    y += 27.0 + 8.0;

    // 列表区：底部按钮行之上。
    let list_bottom = inner.bottom() - BUTTON_H - 8.0;
    let list_rect = egui::Rect::from_min_max(
        egui::pos2(inner.left(), y),
        egui::pos2(inner.right(), list_bottom.max(y)),
    );
    if !model.has_tracks {
        label_block(
            ui,
            egui::Rect::from_min_size(list_rect.min, Vec2::new(list_rect.width(), ROW_H + 8.0)),
            &model.empty,
            14.0,
            theme::text(),
        );
    } else {
        level_list(ui, list_rect, model);
    }

    // 底部「保存」「取消」。
    let mut x = inner.left();
    let save_w = text_width(ui, &model.save, 14.0) + 32.0;
    if button(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(x, inner.bottom() - BUTTON_H),
            Vec2::new(save_w, BUTTON_H),
        ),
        &model.save,
    )
    .clicked()
    {
        push_action(ui.ctx(), BrushSettingsAction::Save);
    }
    x += save_w + BUTTON_GAP;
    let cancel_w = text_width(ui, &model.cancel, 14.0) + 32.0;
    if button(
        ui,
        egui::Rect::from_min_size(
            egui::pos2(x, inner.bottom() - BUTTON_H),
            Vec2::new(cancel_w, BUTTON_H),
        ),
        &model.cancel,
    )
    .clicked()
    {
        push_action(ui.ctx(), BrushSettingsAction::Cancel);
    }
}

/// 逐层列表（可滚动，行距 8）。
fn level_list(ui: &mut egui::Ui, rect: egui::Rect, model: &BrushSettingsModel) {
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
        .id_salt("lumino.brush-settings.levels")
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            ui.spacing_mut().item_spacing.y = ROW_GAP;
            for level in &model.levels {
                level_row(ui, level);
            }
        });
}

/// 一行：层名 + 音轨下拉 + 插入 / 删除。
fn level_row(ui: &mut egui::Ui, level: &crate::chrome::BrushLevelModel) {
    let width = ui.available_width().max(1.0);
    let (row, _) = ui.allocate_exact_size(Vec2::new(width, ROW_H), Sense::hover());
    let mut x = row.left();

    let mut label = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(egui::Rect::from_min_size(
                egui::pos2(x, row.top()),
                Vec2::new(LABEL_W, ROW_H),
            ))
            .layout(egui::Layout::left_to_right(Align::Center)),
    );
    label.add(egui::Label::new(
        RichText::new(&level.label)
            .size(theme::font::LABEL)
            .color(theme::text()),
    ));
    x += LABEL_W + 8.0;

    let selected = level
        .tracks
        .iter()
        .find(|choice| choice.selected)
        .map(|choice| choice.label.as_str())
        .unwrap_or("");
    let combo_rect = egui::Rect::from_min_size(egui::pos2(x, row.top()), Vec2::new(COMBO_W, ROW_H));
    let mut combo = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(combo_rect)
            .layout(egui::Layout::left_to_right(Align::Center)),
    );
    combo.spacing_mut().interact_size.y = ROW_H;
    let mut chosen: Option<Option<usize>> = None;
    egui::ComboBox::from_id_salt(("brush-level", level.index))
        .selected_text(RichText::new(selected).size(theme::font::LABEL))
        .width(COMBO_W - 6.0)
        .show_ui(&mut combo, |ui| {
            for choice in &level.tracks {
                if ui
                    .selectable_label(choice.selected, &choice.label)
                    .clicked()
                {
                    chosen = Some(choice.id);
                }
            }
        });
    if let Some(id) = chosen {
        push_action(
            ui.ctx(),
            BrushSettingsAction::LevelTrackChanged(level.index, id),
        );
    }
    x += COMBO_W + 8.0;

    if icon_button(ui, egui::pos2(x, row.top()), true, true, "添加层").clicked() {
        push_action(ui.ctx(), BrushSettingsAction::AddLevel(level.index));
    }
    x += ICON_BTN + 4.0;
    if icon_button(
        ui,
        egui::pos2(x, row.top()),
        false,
        level.can_remove,
        "删除层",
    )
    .clicked()
        && level.can_remove
    {
        push_action(ui.ctx(), BrushSettingsAction::RemoveLevel(level.index));
    }
}

/// 圆形「+」/「−」图标按钮（19px 圆环 + 横竖线，与 iced 的 PlusCircle 同形）。
fn icon_button(
    ui: &mut egui::Ui,
    origin: egui::Pos2,
    plus: bool,
    enabled: bool,
    label: &str,
) -> egui::Response {
    let rect = egui::Rect::from_min_size(origin, Vec2::splat(ICON_BTN));
    let response = ui.interact(
        rect,
        egui::Id::unique(("brush-icon", rect.left() as i32, plus)),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    // 图标按钮没有可见文字：补 a11y 标签，读屏与无头测试都能定位。
    let info_label = label.to_owned();
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, info_label.clone()));
    let circle = egui::Rect::from_center_size(rect.center(), Vec2::splat(18.0));
    let color = if !enabled {
        theme::text_muted()
    } else if response.hovered() {
        theme::accent()
    } else {
        theme::text()
    };
    let painter = ui.painter();
    painter.circle_stroke(circle.center(), 8.5, Stroke::new(1.5, color));
    painter.line_segment(
        [
            egui::pos2(circle.center().x - 4.0, circle.center().y),
            egui::pos2(circle.center().x + 4.0, circle.center().y),
        ],
        Stroke::new(1.5, color),
    );
    if plus {
        painter.line_segment(
            [
                egui::pos2(circle.center().x, circle.center().y - 4.0),
                egui::pos2(circle.center().x, circle.center().y + 4.0),
            ],
            Stroke::new(1.5, color),
        );
    }
    response
}

fn text_width(ui: &egui::Ui, text: &str, size: f32) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), FontId::proportional(size), theme::text())
        .size()
        .x
        .ceil()
}

fn button(ui: &mut egui::Ui, rect: egui::Rect, label: &str) -> egui::Response {
    let button = egui::Button::new(
        RichText::new(label)
            .size(theme::font::LABEL)
            .color(theme::on_accent()),
    )
    .fill(theme::accent())
    .corner_radius(CornerRadius::same(4));
    ui.put(rect, button)
}

/// 固定矩形内的左对齐文本标签（保留可访问树节点）。
fn label_block(ui: &mut egui::Ui, rect: egui::Rect, text: &str, size: f32, color: Color32) {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(Align::Center)),
    );
    child.add(egui::Label::new(RichText::new(text).size(size).color(color)).selectable(false));
}
