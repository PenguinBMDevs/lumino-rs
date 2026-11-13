//! 设置「云管理」页：告警条 + 连接列表（连接 / 断开 / 管理 / 删除）。
//!
//! 连接行按钮带回连接 id；布局为「名称/地址」左列 + 状态与操作按钮右列。

use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Vec2};

use crate::theme;

use super::super::chrome::{CloudPageModel, SettingsPalette};
use super::action::{self, SettingsAction};
use super::pages::text_width;
use super::set_hits;

const PAD: f32 = 20.0;
const TITLE_H: f32 = 30.0;
const ROW_H: f32 = 52.0;
const BTN_H: f32 = 24.0;
const BTN_GAP: f32 = 8.0;

/// 内容高度估算（供 ScrollArea 预留）。
pub(super) fn content_height(model: &CloudPageModel, width: f32) -> f32 {
    let alert = model
        .alert
        .as_ref()
        .map(|text| {
            let per_line = ((width - PAD * 2.0 - 16.0) / 7.0).max(10.0);
            (text.chars().count() as f32 / per_line).ceil().max(1.0) * 16.0 + 16.0 + 12.0
        })
        .unwrap_or(0.0);
    let rows = model.rows.len().max(1) as f32 * ROW_H;
    PAD * 2.0 + TITLE_H + alert + 42.0 + rows
}

/// 绘制页面。
pub(super) fn show(
    ui: &mut egui::Ui,
    rect: Rect,
    model: &CloudPageModel,
    palette: &SettingsPalette,
) {
    let x = rect.left() + PAD;
    let mut y = rect.top() + PAD;
    ui.painter().text(
        Pos2::new(x, y + 10.0),
        Align2::LEFT_CENTER,
        &model.title,
        FontId::proportional(16.0),
        palette.text,
    );
    y += TITLE_H;

    // 告警条：按实际宽度换行。
    if let Some(alert) = &model.alert {
        let wrap = (rect.right() - PAD - x).max(80.0);
        let dark = palette.background.r() < 128 && palette.background.g() < 128;
        let (bg, fg) = if dark {
            (
                Color32::from_rgb(0x4A, 0x2A, 0x33),
                Color32::from_rgb(0xFF, 0x8A, 0x8A),
            )
        } else {
            (
                Color32::from_rgb(0xFD, 0xE7, 0xE9),
                Color32::from_rgb(0xB4, 0x23, 0x18),
            )
        };
        let galley =
            ui.painter()
                .layout(alert.clone(), FontId::proportional(12.0), fg, wrap - 16.0);
        let banner = Rect::from_min_size(
            Pos2::new(x, y),
            Vec2::new(rect.width() - PAD * 2.0, galley.size().y + 16.0),
        );
        ui.painter().rect_filled(banner, CornerRadius::same(4), bg);
        ui.painter().galley(
            Pos2::new(banner.left() + 8.0, banner.top() + 8.0),
            galley,
            fg,
        );
        y = banner.bottom() + 12.0;
    }

    // 添加连接按钮。
    let add_w = text_width(ui, &model.add_label, 13.0) + 36.0;
    let add = Rect::from_min_size(Pos2::new(x, y), Vec2::new(add_w, 30.0));
    let response = ui.interact(add, egui::Id::unique("cloud-add"), Sense::click());
    ui.painter().rect_filled(
        add,
        CornerRadius::same(6),
        if response.hovered() {
            palette.accent_strong
        } else {
            palette.accent
        },
    );
    ui.painter().text(
        add.center(),
        Align2::CENTER_CENTER,
        &model.add_label,
        FontId::proportional(13.0),
        theme::on_accent(),
    );
    if response.clicked() {
        action::push_action(ui.ctx(), SettingsAction::CloudAdd);
    }
    let mut hits = super::hits(ui.ctx());
    hits.cloud_buttons.push(("add".to_owned(), add));
    set_hits(ui.ctx(), hits);
    y += 42.0;

    if model.rows.is_empty() {
        ui.painter().text(
            Pos2::new(x, y + 6.0),
            Align2::LEFT_CENTER,
            &model.empty_text,
            FontId::proportional(12.0),
            palette.text_dim,
        );
        return;
    }

    for row in &model.rows {
        let row_rect =
            Rect::from_min_size(Pos2::new(x, y), Vec2::new(rect.width() - PAD * 2.0, ROW_H));
        ui.painter().text(
            Pos2::new(row_rect.left(), row_rect.top() + 12.0),
            Align2::LEFT_CENTER,
            &row.title,
            FontId::proportional(13.0),
            palette.text,
        );
        ui.painter().text(
            Pos2::new(row_rect.left(), row_rect.top() + 30.0),
            Align2::LEFT_CENTER,
            &row.address,
            FontId::proportional(11.0),
            palette.text_dim,
        );

        // 右侧操作区：从右往左排「删除 / 管理文件 / 连接(断开)」。
        let mut right = row_rect.right();
        let button = |ui: &mut egui::Ui,
                      right: &mut f32,
                      label: &str,
                      enabled: bool,
                      key: String,
                      action: SettingsAction,
                      palette: &SettingsPalette| {
            let width = text_width(ui, label, 12.0) + 20.0;
            *right -= width;
            let rect = Rect::from_min_size(
                Pos2::new(*right, row_rect.center().y - BTN_H / 2.0),
                Vec2::new(width, BTN_H),
            );
            let response = ui.interact(
                rect,
                egui::Id::unique(("cloud-btn", key.as_str())),
                if enabled {
                    Sense::click()
                } else {
                    Sense::hover()
                },
            );
            if enabled && response.hovered() {
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::same(4),
                    Color32::from_white_alpha(20),
                );
            }
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(12.0),
                if enabled {
                    palette.text
                } else {
                    palette.text_dim
                },
            );
            if enabled && response.clicked() {
                action::push_action(ui.ctx(), action);
            }
            let mut hits = super::hits(ui.ctx());
            hits.cloud_buttons.push((key, rect));
            set_hits(ui.ctx(), hits);
            *right -= BTN_GAP;
        };

        button(
            ui,
            &mut right,
            &row.delete_label,
            true,
            format!("delete:{}", row.id),
            SettingsAction::CloudDelete(row.id.clone()),
            palette,
        );
        button(
            ui,
            &mut right,
            &row.manage_label,
            row.online,
            format!("manage:{}", row.id),
            SettingsAction::CloudManage,
            palette,
        );
        let connect_action = if row.online {
            SettingsAction::CloudDisconnect(row.id.clone())
        } else {
            SettingsAction::CloudConnect(row.id.clone())
        };
        button(
            ui,
            &mut right,
            &row.connect_label,
            true,
            format!("connect:{}", row.id),
            connect_action,
            palette,
        );

        // 状态标记：位于按钮左侧（圆点自绘，避免 emoji 缺字形）。
        let status_color = if row.online {
            Color32::from_rgb(0x4C, 0xAF, 0x50)
        } else {
            Color32::from_rgb(0xE5, 0x48, 0x4D)
        };
        let dot = Pos2::new(right - 30.0, row_rect.center().y);
        ui.painter().circle_filled(dot, 3.5, status_color);
        ui.painter().text(
            Pos2::new(right, row_rect.center().y),
            Align2::RIGHT_CENTER,
            &row.status,
            FontId::proportional(12.0),
            palette.text_dim,
        );

        y += ROW_H;
    }
}
