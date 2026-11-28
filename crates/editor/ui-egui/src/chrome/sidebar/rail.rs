//! 右侧栏图标列：48px 宽、按钮 48×48 + 2px 间距、灯条贴右边缘。
//!
//! 图标列由 egui **独占**：绘制 + 点击 + 动作通道（`RailClicked`）。
//! 因此整列计入占用区，否则同一次点击会同时被 egui 与画布各处理一次。

use egui::{Color32, Rect, Vec2};

use crate::chrome::{RightSidebarModel, RightSidebarPanelKind};
use crate::icons::Icon;
use crate::theme;

/// 右侧栏按钮边长与间距（生产 `Column::spacing(2)`）。
const RIGHT_BTN: f32 = 48.0;
const RIGHT_BTN_GAP: f32 = 2.0;
/// 右侧栏图标尺寸（生产 20×20）。
const RIGHT_ICON: f32 = 20.0;

/// 右侧栏图标列。
pub fn show_right_model(
    ui: &mut egui::Ui,
    model: &RightSidebarModel,
    icons: &mut crate::icons::IconCache,
) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, theme::route_bar_bg());
    let left = rect.left();
    // 生产侧总会给出三项（含 i18n 文案）；测试构造的模型可能为空，回退到内置顺序。
    let fallback = [
        (RightSidebarPanelKind::ImageToMidi, Icon::ImageToMidi, ""),
        (RightSidebarPanelKind::Materials, Icon::MaterialLibrary, ""),
        (
            RightSidebarPanelKind::PianoWaterfall,
            Icon::PianoWaterfall,
            "",
        ),
    ];
    let items: Vec<(RightSidebarPanelKind, Icon, &str)> = if model.rail.is_empty() {
        fallback
            .iter()
            .map(|(kind, icon, label)| (*kind, *icon, *label))
            .collect()
    } else {
        model
            .rail
            .iter()
            .map(|item| {
                let icon = match item.kind {
                    RightSidebarPanelKind::ImageToMidi => Icon::ImageToMidi,
                    RightSidebarPanelKind::Materials => Icon::MaterialLibrary,
                    RightSidebarPanelKind::PianoWaterfall => Icon::PianoWaterfall,
                };
                (item.kind, icon, item.label.as_str())
            })
            .collect()
    };

    for (index, (kind, icon, label)) in items.into_iter().enumerate() {
        let y = rect.top() + index as f32 * (RIGHT_BTN + RIGHT_BTN_GAP);
        right_button(ui, icons, icon, left, y, model.active, Some((kind, label)));
    }
    // 整列占用：点击只给 egui（画布不再收到同位置事件）。
    crate::chrome::menu::extend_blocked(ui.ctx(), [rect]);
}

/// 单个右侧栏按钮：图标在 46px 内容区居中（灯条在右边缘 2px）。
fn right_button(
    ui: &egui::Ui,
    icons: &mut crate::icons::IconCache,
    icon: Icon,
    left: f32,
    y: f32,
    active: Option<RightSidebarPanelKind>,
    route: Option<(RightSidebarPanelKind, &str)>,
) {
    let rect = Rect::from_min_size(egui::pos2(left, y), Vec2::splat(RIGHT_BTN));
    let lit = matches!(
        (active, icon),
        (Some(RightSidebarPanelKind::ImageToMidi), Icon::ImageToMidi)
            | (
                Some(RightSidebarPanelKind::Materials),
                Icon::MaterialLibrary
            )
            | (
                Some(RightSidebarPanelKind::PianoWaterfall),
                Icon::PianoWaterfall
            )
    );
    if let Some((kind, label)) = route {
        let response = ui.interact(
            rect,
            egui::Id::unique(("lumino.right-rail", y as i32)),
            egui::Sense::click(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::Button, true, lit, format!("{kind:?}"))
        });
        if response.hovered() {
            ui.painter()
                .rect_filled(rect, 0, Color32::from_white_alpha(10));
        }
        if response.clicked() {
            crate::chrome::right_panel::push_action(
                ui.ctx(),
                crate::chrome::right_panel::RightSidebarAction::RailClicked(kind),
            );
        }
        if !label.is_empty() {
            response.on_hover_text(label);
        }
    }
    // 灯条画在悬停底色之后，避免被覆盖。
    if lit {
        ui.painter().rect_filled(
            Rect::from_min_size(
                egui::pos2(rect.right() - 2.0, rect.top()),
                Vec2::new(2.0, rect.height()),
            ),
            0,
            theme::accent(),
        );
    }
    // 内容区 = 48 - 2（灯条），图标 20 居中 → 起点 = 左 + 13。
    let icon_rect = Rect::from_min_size(
        egui::pos2(rect.left() + 13.0, rect.center().y - RIGHT_ICON / 2.0),
        Vec2::splat(RIGHT_ICON),
    );
    icons.paint(ui.painter(), icon, icon_rect);
}
