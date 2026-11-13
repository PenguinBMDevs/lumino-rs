//! 撤销历史对话框（`DialogType::History`）。
//!
//! 只读时间线：初始状态 + 每个步骤（最老在上），当前游标行高亮、未来步骤浅色；
//! 点击任意行回传 `Jump(index)`，底部「关闭」回传 `Close`。
//!
//! 几何对齐生产 `view/history_dialog.rs`：整窗 base 底 + 1px `strong` 边框
//! （圆角 8）、内边距 16、标题 18px、提示 12px、行高 32 + 间距 2、底部计数与
//! 关闭按钮；列表在 8px 实体滚动条内滚动。

use egui::{Color32, CornerRadius, Id, Rect, RichText, ScrollArea, Vec2};

use crate::chrome::HistoryModel;
use crate::theme;

const ACTION_ID: &str = "lumino.history.action";
/// 面板内边距（生产 `padding(16)`）。
const PAD: f32 = 16.0;
/// 行高与行间距（生产 `ROW_HEIGHT = 32`、`spacing(2)`）。
const ROW_H: f32 = 32.0;
const ROW_GAP: f32 = 2.0;
/// 标题 / 提示字号。
const TITLE_SIZE: f32 = 18.0;
const HINT_SIZE: f32 = 12.0;
/// 关闭按钮尺寸（生产 `padding([6, 18])` + 13px 文本）。
const CLOSE_W: f32 = 64.0;
const CLOSE_H: f32 = 28.0;

/// 撤销历史对话框产生的动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryAction {
    /// 跳到第 `index` 步之后的状态（0 = 初始状态）。
    Jump(usize),
    /// 关闭对话框。
    Close,
}

/// 记录本帧动作（一帧只保留第一个）。
pub fn push_action(ctx: &egui::Context, action: HistoryAction) {
    let pending = ctx
        .data(|d| d.get_temp::<Option<HistoryAction>>(Id::unique(ACTION_ID)))
        .flatten()
        .is_some();
    if pending {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<HistoryAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<HistoryAction>>(Id::unique(ACTION_ID)))
        .flatten()
}

/// 渲染整个对话框窗口（`ui` = 客户端区域）。
pub fn show(ui: &mut egui::Ui, model: &HistoryModel, icons: &mut crate::icons::IconCache) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());

    let rect = ui.max_rect();
    // 顶部自制标题栏仍由 旧版 绘制（红绿灯 / 拖动）。
    let top = if model.titlebar {
        crate::layout::TITLEBAR_H
    } else {
        0.0
    };
    let panel = Rect::from_min_max(egui::pos2(rect.left(), rect.top() + top), rect.max);
    if panel.width() <= 0.0 || panel.height() <= 0.0 {
        return;
    }
    let painter = ui.painter();
    painter.rect_filled(panel, 0, theme::base_bg());

    let inner = panel.shrink(PAD);
    if inner.width() <= 0.0 || inner.height() <= 0.0 {
        return;
    }

    // 标题 + 提示（标题行高 22；间距 6 + `space(4)`；提示行高 15）。
    put_label(
        ui,
        Rect::from_min_size(
            egui::pos2(inner.left(), inner.top()),
            Vec2::new(inner.width(), 22.0),
        ),
        &model.title,
        TITLE_SIZE,
        theme::text(),
        egui::Align::TOP,
    );
    let hint_y = inner.top() + 22.0 + 10.0;
    put_label(
        ui,
        Rect::from_min_size(
            egui::pos2(inner.left(), hint_y),
            Vec2::new(inner.width(), 15.0),
        ),
        &model.hint,
        HINT_SIZE,
        theme::text_muted(),
        egui::Align::TOP,
    );

    // 底部：计数（左）+ 关闭按钮（右）。
    let footer_y = inner.bottom() - CLOSE_H;
    put_label(
        ui,
        Rect::from_min_size(
            egui::pos2(inner.left(), footer_y),
            Vec2::new((inner.width() - CLOSE_W - 8.0).max(1.0), CLOSE_H),
        ),
        &model.counter,
        HINT_SIZE,
        theme::text_muted(),
        egui::Align::Center,
    );
    let close_rect = Rect::from_min_size(
        egui::pos2(inner.right() - CLOSE_W, footer_y),
        Vec2::new(CLOSE_W, CLOSE_H),
    );
    let close = egui::Button::new(
        RichText::new(&model.close)
            .size(13.0)
            .color(theme::on_accent()),
    )
    .fill(theme::accent())
    .corner_radius(CornerRadius::same(4));
    if ui.put(close_rect, close).clicked() {
        push_action(ui.ctx(), HistoryAction::Close);
    }

    // 列表：提示下方 8 + 6，底部到关闭按钮上方 6。
    let list_rect = Rect::from_min_max(
        egui::pos2(inner.left(), hint_y + 15.0 + 14.0),
        egui::pos2(inner.right(), (footer_y - 6.0).max(hint_y + 15.0 + 14.0)),
    );
    if list_rect.width() <= 0.0 || list_rect.height() <= 0.0 {
        return;
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(list_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
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

    ScrollArea::vertical()
        .id_salt("lumino.history.scroll")
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            ui.spacing_mut().item_spacing.y = ROW_GAP;
            let width = ui.available_width().max(1.0);
            for row in &model.rows {
                if row_button(ui, row, width, icons) {
                    push_action(ui.ctx(), HistoryAction::Jump(row.index));
                }
            }
        });
}

/// 单行：当前行主色弱底，未来步骤浅色文本；当前步骤用 `Undo` 图标标记。
fn row_button(
    ui: &mut egui::Ui,
    row: &crate::chrome::HistoryRowModel,
    width: f32,
    icons: &mut crate::icons::IconCache,
) -> bool {
    let color = if row.future {
        theme::text_muted()
    } else {
        theme::text()
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, ROW_H), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &row.label));

    let fill = if row.current {
        theme::accent_dark()
    } else if response.hovered() {
        // 生产：悬停叠 6% 白（`Color::from_rgba(1,1,1,0.06)`）。
        Color32::from_rgba_unmultiplied(255, 255, 255, 15)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, CornerRadius::same(4), fill);

    // DEBT-13：当前步骤标记改用图标（history-undo），不再用「▶」（U+25B6 在部分
    // 字体/平台按 Emoji 呈现）。非当前行保留等宽空位，切换步骤时文字不跳动。
    const MARKER: f32 = 16.0;
    if row.current {
        let icon_rect = Rect::from_min_size(
            egui::pos2(rect.left() + 8.0, rect.center().y - MARKER * 0.5),
            Vec2::splat(MARKER),
        );
        icons.paint(ui.painter(), crate::icons::Icon::Undo, icon_rect);
    }
    ui.painter().text(
        egui::pos2(rect.left() + 8.0 + MARKER + 6.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        &row.label,
        egui::FontId::proportional(13.0),
        color,
    );
    response.clicked()
}

/// 在固定矩形内放一个左对齐（或垂直居中）的文本标签：保留可访问树节点。
fn put_label(
    ui: &mut egui::Ui,
    rect: Rect,
    text: &str,
    size: f32,
    color: Color32,
    align: egui::Align,
) {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(align)),
    );
    child.add(egui::Label::new(RichText::new(text).size(size).color(color)).selectable(false));
}
