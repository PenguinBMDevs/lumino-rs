//! 找回删除音轨对话框（`DialogType::RecoverTrack`）。
//!
//! 内边距 24、列间距 4：标题 18px、12px 提示；36 高表头（`background.weak` 底 +
//! 1px `strong` 边框、圆角 4）三列（文件名 240 / 删除时间 180 / 音符总数右对齐）；
//! 400 高列表区（`base` 底 + 1px 边框，行高 36、行距 2，悬停 `weak` 底、
//! 选中 `primary.base` 底 + `on_accent` 字）；底部右对齐「永久删除」「恢复」
//! 「关闭」（各 110×34，未选中时前两个置灰）。
//!
//! 实际磁盘 I/O 由 Runner 完成，本层只回传动作。

use egui::{Align, Color32, CornerRadius, RichText, Sense, Stroke, Vec2};

use crate::chrome::RecoverTrackModel;
use crate::theme;

const ACTION_ID: &str = "lumino.recover-track.action";
/// 内边距（生产 `container(...).padding(24)`）。
const PAD: f32 = 24.0;
/// 列宽（生产 `COL_FILENAME` / `COL_DELETED_AT`）。
const COL_FILENAME: f32 = 240.0;
const COL_DELETED_AT: f32 = 180.0;
/// 行高（生产 `ROW_HEIGHT`）。
const ROW_H: f32 = 36.0;
/// 行间距（生产 `column(list).spacing(2)`）。
const ROW_GAP: f32 = 2.0;
/// 列表区固定高度（生产 `LIST_MAX_HEIGHT`）。
const LIST_H: f32 = 400.0;
/// 单元格内边距（生产 `padding([6,8])`）。
const CELL_PAD_X: f32 = 8.0;
/// 底部按钮（生产 110 宽、`padding([8,24])`）。
const BUTTON_W: f32 = 110.0;
const BUTTON_H: f32 = 34.0;
const BUTTON_GAP: f32 = 8.0;
/// 空态行高（生产 80）。
const EMPTY_H: f32 = 80.0;

/// 找回删除音轨对话框产生的动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoverTrackAction {
    /// 选中第 `index` 行。
    SelectionChanged(usize),
    /// 恢复选中条目。
    Restore {
        /// 缓存文件路径。
        path: String,
        /// 原轨道编号。
        original_index: usize,
    },
    /// 永久删除选中条目。
    PermanentlyDelete {
        /// 缓存文件路径。
        path: String,
        /// 轨道编号。
        track_id: u16,
    },
    /// 关闭对话框。
    CloseDialog,
}

/// 记录本帧动作（一帧只保留第一个）。
pub fn push_action(ctx: &egui::Context, action: RecoverTrackAction) {
    if ctx
        .data(|d| d.get_temp::<Option<RecoverTrackAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
        .is_some()
    {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<RecoverTrackAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<RecoverTrackAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
}

/// 渲染整个对话框窗口（`ui` = 客户端区域）。
pub fn show(ui: &mut egui::Ui, model: &RecoverTrackModel) {
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

    // 标题（18px，行高 27）+ 4 + 提示（12px，行高 18）+ 4。
    let mut y = inner.top();
    label_left(
        ui,
        egui::Rect::from_min_size(egui::pos2(inner.left(), y), Vec2::new(inner.width(), 27.0)),
        &model.title,
        18.0,
        theme::text(),
    );
    y += 27.0 + 8.0;
    label_left(
        ui,
        egui::Rect::from_min_size(egui::pos2(inner.left(), y), Vec2::new(inner.width(), 18.0)),
        &model.hint,
        12.0,
        theme::text_muted(),
    );
    y += 18.0 + 16.0;

    // 表头。
    let header =
        egui::Rect::from_min_size(egui::pos2(inner.left(), y), Vec2::new(inner.width(), ROW_H));
    ui.painter().rect(
        header,
        CornerRadius::same(4),
        theme::toolbar_bg(),
        Stroke::new(1.0, theme::selected_bg()),
        egui::StrokeKind::Inside,
    );
    cell_text(
        ui,
        header,
        0.0,
        COL_FILENAME,
        &model.filename_header,
        Align::Min,
    );
    cell_text(
        ui,
        header,
        COL_FILENAME,
        COL_DELETED_AT,
        &model.deleted_header,
        Align::Min,
    );
    cell_text(
        ui,
        header,
        COL_FILENAME + COL_DELETED_AT,
        header.width() - COL_FILENAME - COL_DELETED_AT,
        &model.notes_header,
        Align::Max,
    );
    y += ROW_H + 8.0;

    // 列表区（固定 400 高 + 边框）。
    let list = egui::Rect::from_min_size(
        egui::pos2(inner.left(), y),
        Vec2::new(
            inner.width(),
            LIST_H
                .min(inner.height() - (y - inner.top()) - 20.0 - BUTTON_H)
                .max(ROW_H),
        ),
    );
    ui.painter().rect(
        list,
        CornerRadius::same(4),
        theme::base_bg(),
        Stroke::new(1.0, theme::selected_bg()),
        egui::StrokeKind::Inside,
    );
    list_rows(ui, list.shrink(1.0), model);
    y = list.bottom() + 16.0 + 4.0;

    // 底部按钮行：右对齐 永久删除 / 恢复 / 关闭。
    let mut x = inner.right() - BUTTON_W;
    if button(ui, x, y, &model.close, ButtonKind::Neutral).clicked() {
        push_action(ui.ctx(), RecoverTrackAction::CloseDialog);
    }
    x -= BUTTON_GAP + BUTTON_W;
    if button(ui, x, y, &model.restore, ButtonKind::Restore(model.can_act)).clicked()
        && let Some(entry) = &model.selected
    {
        push_action(
            ui.ctx(),
            RecoverTrackAction::Restore {
                path: entry.path.clone(),
                original_index: entry.original_index,
            },
        );
    }
    x -= BUTTON_GAP + BUTTON_W;
    if button(ui, x, y, &model.delete, ButtonKind::Danger(model.can_act)).clicked()
        && let Some(entry) = &model.selected
    {
        push_action(
            ui.ctx(),
            RecoverTrackAction::PermanentlyDelete {
                path: entry.path.clone(),
                track_id: entry.track_id,
            },
        );
    }
}

/// 列表内容（可滚动，行距 2；空态 80 高居中）。
fn list_rows(ui: &mut egui::Ui, rect: egui::Rect, model: &RecoverTrackModel) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
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

    if model.entries.is_empty() {
        let mut empty = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(egui::Rect::from_min_size(
                    rect.min,
                    Vec2::new(rect.width(), EMPTY_H),
                ))
                .layout(egui::Layout::centered_and_justified(
                    egui::Direction::LeftToRight,
                )),
        );
        empty.add(egui::Label::new(
            RichText::new(&model.empty)
                .size(13.0)
                .color(theme::text_muted()),
        ));
        return;
    }

    egui::ScrollArea::vertical()
        .id_salt("lumino.recover-track.list")
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            ui.spacing_mut().item_spacing.y = ROW_GAP;
            let width = ui.available_width().max(1.0);
            for (index, entry) in model.entries.iter().enumerate() {
                let (row, response) =
                    ui.allocate_exact_size(Vec2::new(width, ROW_H), Sense::click());
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Button, true, &entry.filename)
                });
                if entry.selected {
                    ui.painter()
                        .rect_filled(row, CornerRadius::same(4), theme::accent());
                } else if response.hovered() {
                    ui.painter()
                        .rect_filled(row, CornerRadius::same(4), theme::toolbar_bg());
                }
                let color = if entry.selected {
                    theme::on_accent()
                } else {
                    theme::text()
                };
                cell_text_color(
                    ui,
                    row,
                    0.0,
                    COL_FILENAME,
                    &entry.filename,
                    Align::Min,
                    color,
                );
                cell_text_color(
                    ui,
                    row,
                    COL_FILENAME,
                    COL_DELETED_AT,
                    &entry.deleted_at,
                    Align::Min,
                    color,
                );
                cell_text_color(
                    ui,
                    row,
                    COL_FILENAME + COL_DELETED_AT,
                    row.width() - COL_FILENAME - COL_DELETED_AT,
                    &entry.note_count,
                    Align::Max,
                    color,
                );
                if response.clicked() {
                    push_action(ui.ctx(), RecoverTrackAction::SelectionChanged(index));
                }
            }
        });
}

/// 单元格文本（左 / 右对齐，内边距 8，进入可访问树）。
fn cell_text(
    ui: &mut egui::Ui,
    row: egui::Rect,
    offset: f32,
    width: f32,
    text: &str,
    align: Align,
) {
    cell_text_color(ui, row, offset, width, text, align, theme::text());
}

fn cell_text_color(
    ui: &mut egui::Ui,
    row: egui::Rect,
    offset: f32,
    width: f32,
    text: &str,
    align: Align,
    color: Color32,
) {
    let pad = CELL_PAD_X;
    let left = row.left() + offset + pad;
    let right = row.left() + offset + width - pad;
    if right <= left {
        return;
    }
    let rect =
        egui::Rect::from_min_max(egui::pos2(left, row.top()), egui::pos2(right, row.bottom()));
    let layout = match align {
        Align::Max => egui::Layout::right_to_left(Align::Center),
        _ => egui::Layout::left_to_right(Align::Center),
    };
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(layout));
    child.add(egui::Label::new(RichText::new(text).size(13.0).color(color)).selectable(false));
}

/// 按钮种类（决定底色与置灰）。
enum ButtonKind {
    /// 关闭（`weak` 底、正文色）。
    Neutral,
    /// 恢复（主色底，未选中置灰）。
    Restore(bool),
    /// 永久删除（危险色底，未选中置灰）。
    Danger(bool),
}

fn button(ui: &mut egui::Ui, x: f32, y: f32, label: &str, kind: ButtonKind) -> egui::Response {
    let rect = egui::Rect::from_min_size(egui::pos2(x, y), Vec2::new(BUTTON_W, BUTTON_H));
    let (fill, text) = match kind {
        ButtonKind::Neutral => (theme::toolbar_bg(), theme::text()),
        ButtonKind::Restore(true) => (theme::accent(), theme::on_accent()),
        ButtonKind::Danger(true) => (theme::danger(), Color32::WHITE),
        ButtonKind::Restore(false) | ButtonKind::Danger(false) => {
            (theme::toolbar_bg(), theme::text_muted())
        }
    };
    let button = egui::Button::new(RichText::new(label).size(theme::font::LABEL).color(text))
        .fill(fill)
        .corner_radius(CornerRadius::same(4));
    ui.put(rect, button)
}

/// 固定矩形内的左对齐文本标签（保留可访问树节点）。
fn label_left(ui: &mut egui::Ui, rect: egui::Rect, text: &str, size: f32, color: Color32) {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(Align::Center)),
    );
    child.add(egui::Label::new(RichText::new(text).size(size).color(color)).selectable(false));
}
