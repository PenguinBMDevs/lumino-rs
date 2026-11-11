//! 音轨取色器 / 端口·通道选择器。
//!
//! 几何与 iced `sidebar/color_picker.rs` / `numeric_picker.rs` 一致：
//! - 取色器：色块直径 20、间距 4、每行 4 个（8 色 + 1 行「恢复默认」）、内边距 8；
//! - 数值选择器：按钮 30×30、间距 4、每行 4 个（16 项 → 4×4）、内边距 8；
//! - 面板右对齐到面板内容右缘 −8、纵向贴目标行顶部 +8；
//! - 底色 `rgba(0.06,0.06,0.08,0.96)`、圆角 8、当前值用强调色、悬停白 12%。
//!
//! 元素必须画在**与面板底同一层**（`egui::Area` 内），否则浮层会抢走命中。

use egui::{Color32, CornerRadius, Rect, Sense, Stroke, StrokeKind, Vec2};

use super::super::{SidebarColorPickerModel, SidebarNumericPickerKind, SidebarNumericPickerModel};
use super::panel_action::{self, SidebarPanelAction};

/// 色块直径。
const SWATCH: f32 = 20.0;
/// 数值按钮边长。
const ITEM: f32 = 30.0;
/// 元素间距。
const SPACING: f32 = 4.0;
/// 每行元素数。
const PER_ROW: usize = 4;
/// 面板内边距。
const PAD: f32 = 8.0;
/// 面板与目标行的纵向偏移。
const OFFSET_Y: f32 = 8.0;
/// 面板与内容右缘的水平间距。
const RIGHT_MARGIN: f32 = 8.0;

/// 预设音轨颜色（与 iced `TRACK_COLORS` 逐项一致）。
pub const TRACK_COLORS: [Color32; 8] = [
    Color32::from_rgb(217, 38, 38),
    Color32::from_rgb(38, 191, 89),
    Color32::from_rgb(38, 115, 217),
    Color32::from_rgb(217, 191, 26),
    Color32::from_rgb(191, 38, 191),
    Color32::from_rgb(38, 191, 191),
    Color32::from_rgb(242, 128, 38),
    Color32::from_rgb(128, 128, 128),
];

/// 面板底色。
fn panel_bg() -> Color32 {
    Color32::from_rgba_unmultiplied(15, 15, 20, 245)
}

/// 悬停底色（白色 12%）。
fn hover_bg() -> Color32 {
    Color32::from_white_alpha(31)
}

/// 面板尺寸（每行最多 4 个）。
fn panel_size(count: usize, item: f32) -> Vec2 {
    let rows = count.div_ceil(PER_ROW);
    let cols = count.min(PER_ROW);
    Vec2::new(
        cols as f32 * item + (cols.saturating_sub(1)) as f32 * SPACING + PAD * 2.0,
        rows as f32 * item + rows.saturating_sub(1) as f32 * SPACING + PAD * 2.0,
    )
}

/// 面板左上角：右对齐到内容右缘 −8，纵向贴目标行 +8。
fn panel_pos(content: Rect, anchor: Rect, size: Vec2) -> egui::Pos2 {
    egui::pos2(
        (content.right() - RIGHT_MARGIN - size.x).max(content.left()),
        anchor.top() + OFFSET_Y,
    )
}

/// 第 `index` 个元素在面板内的矩形。
fn cell(origin: egui::Pos2, index: usize, item: f32) -> Rect {
    Rect::from_min_size(
        egui::pos2(
            origin.x + PAD + (index % PER_ROW) as f32 * (item + SPACING),
            origin.y + PAD + (index / PER_ROW) as f32 * (item + SPACING),
        ),
        Vec2::splat(item),
    )
}

/// 整块面板计入占用（iced 的同位面板不再处理同一次点击）+ 背景关闭判定。
fn blocked_and_backdrop(
    ui: &egui::Ui,
    content: Rect,
    pos: egui::Pos2,
    size: Vec2,
    id: &str,
) -> bool {
    crate::chrome::menu::extend_blocked(ui.ctx(), [Rect::from_min_size(pos, size)]);
    ui.interact(content, egui::Id::unique((id, "backdrop")), Sense::click())
        .clicked()
}

/// 在浮层里画面板底 + 元素（元素与底同层，避免命中被浮层抢走）。
fn in_area<R>(
    ui: &egui::Ui,
    id: &str,
    pos: egui::Pos2,
    size: Vec2,
    add: impl FnOnce(&mut egui::Ui, Rect) -> R,
) -> R {
    egui::Area::new(egui::Id::unique(id))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .show(ui.ctx(), |ui| {
            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
            ui.painter().rect(
                rect,
                CornerRadius::same(8),
                panel_bg(),
                Stroke::NONE,
                StrokeKind::Inside,
            );
            add(ui, rect)
        })
        .inner
}

/// 音轨取色器；返回本帧动作。
pub(super) fn color_picker(
    ui: &egui::Ui,
    content: Rect,
    anchor: Rect,
    model: &SidebarColorPickerModel,
) -> Option<SidebarPanelAction> {
    // 8 色排 2 行 + 1 行「恢复默认」。
    let count = TRACK_COLORS.len() + 1;
    let size = panel_size(count, SWATCH);
    let pos = panel_pos(content, anchor, size);
    let id = "lumino.sidebar.color-picker";
    let closed = blocked_and_backdrop(ui, content, pos, size, id);

    let clicked = in_area(ui, id, pos, size, |ui, rect| {
        let mut clicked = None;
        for (index, color) in TRACK_COLORS.iter().enumerate() {
            let cell = cell(rect.min, index, SWATCH);
            let response = ui.interact(cell, ui.scope_id().with(("swatch", index)), Sense::click());
            ui.painter()
                .circle_filled(cell.center(), SWATCH / 2.0, *color);
            if response.hovered() {
                ui.painter().circle_stroke(
                    cell.center(),
                    SWATCH / 2.0,
                    Stroke::new(1.0, Color32::WHITE),
                );
            }
            if response.clicked() {
                clicked = Some(SidebarPanelAction::ColorPicked {
                    track_id: model.track_id,
                    rgb: (color.r(), color.g(), color.b()),
                });
            }
        }

        // 「恢复默认」：灰边圆 + 红色斜杠（与 iced `reset_button` 同观感）。
        let reset = cell(rect.min, TRACK_COLORS.len(), SWATCH);
        let response = ui.interact(reset, ui.scope_id().with("reset"), Sense::click());
        ui.painter().circle_stroke(
            reset.center(),
            SWATCH / 2.0,
            Stroke::new(1.0, Color32::from_gray(179)),
        );
        if response.hovered() {
            ui.painter()
                .circle_filled(reset.center(), SWATCH / 2.0, hover_bg());
        }
        let inset = SWATCH / 2.0 - 3.0;
        ui.painter().line_segment(
            [
                egui::pos2(reset.center().x - inset, reset.center().y + inset),
                egui::pos2(reset.center().x + inset, reset.center().y - inset),
            ],
            Stroke::new(2.0, Color32::from_rgb(230, 26, 26)),
        );
        if response.clicked() {
            clicked = Some(SidebarPanelAction::ColorReset(model.track_id));
        }
        clicked
    });

    clicked.or(closed.then_some(SidebarPanelAction::ColorPickerClosed(model.track_id)))
}

/// 端口 / 通道选择器；返回本帧动作。
pub(super) fn numeric_picker(
    ui: &egui::Ui,
    content: Rect,
    anchor: Rect,
    model: &SidebarNumericPickerModel,
) -> Option<SidebarPanelAction> {
    let count = model.choices as usize;
    let size = panel_size(count, ITEM);
    let pos = panel_pos(content, anchor, size);
    let id = match model.kind {
        SidebarNumericPickerKind::Port => "lumino.sidebar.port-picker",
        SidebarNumericPickerKind::Channel => "lumino.sidebar.channel-picker",
    };
    let closed = blocked_and_backdrop(ui, content, pos, size, id);

    let clicked = in_area(ui, id, pos, size, |ui, rect| {
        let mut clicked = None;
        for value in 0..count {
            let cell = cell(rect.min, value, ITEM);
            let response = ui.interact(cell, ui.scope_id().with(("option", value)), Sense::click());
            let selected = value == model.current as usize;
            if selected {
                ui.painter().rect_filled(
                    cell,
                    CornerRadius::same(4),
                    crate::theme::accent_strong(),
                );
            } else if response.hovered() {
                ui.painter()
                    .rect_filled(cell, CornerRadius::same(4), hover_bg());
            }
            ui.painter().text(
                cell.center(),
                egui::Align2::CENTER_CENTER,
                (value + 1).to_string(),
                egui::FontId::proportional(13.0),
                Color32::from_rgb(242, 242, 242),
            );
            if response.clicked() {
                clicked = Some(match model.kind {
                    SidebarNumericPickerKind::Port => SidebarPanelAction::PortPicked {
                        track_id: model.track_id,
                        value: value as u8,
                    },
                    SidebarNumericPickerKind::Channel => SidebarPanelAction::ChannelPicked {
                        track_id: model.track_id,
                        value: value as u8,
                    },
                });
            }
        }
        clicked
    });

    clicked.or(closed.then_some(match model.kind {
        SidebarNumericPickerKind::Port => SidebarPanelAction::PortPickerClosed(model.track_id),
        SidebarNumericPickerKind::Channel => {
            SidebarPanelAction::ChannelPickerClosed(model.track_id)
        }
    }))
}

/// 便捷：把选择器动作写进通道。
pub(super) fn emit(ctx: &egui::Context, action: Option<SidebarPanelAction>) {
    if let Some(action) = action {
        panel_action::push_panel_action(ctx, action);
    }
}
