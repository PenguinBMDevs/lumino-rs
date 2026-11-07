//! 左侧音轨栏 / 右侧图标栏。
//!
//! 路由栏条目与生产 `ROUTES` 一一对应：分组父按钮激活时点亮 `parent_color`，
//! 子路由激活时点亮 `child_color`，其余条目只显示图标。

use egui::{Align, Color32, Layout, Sense, Vec2};
use egui_extras::{Column, Size, StripBuilder, TableBuilder};

use super::{ChromeState, TrackRow};
use crate::icons::Icon;
use crate::layout;
use crate::theme;
use crate::widgets;

/// 路由条目：图标 + 激活时的指示条颜色。
struct RouteEntry {
    icon: Icon,
    active_color: Option<Color32>,
}

/// PianoRoll 分组父色（`GroupId::PianoRoll::parent_color`）。
const PIANO_ROLL_PARENT: Color32 = Color32::from_rgb(0xD9, 0x26, 0x26);
/// PianoRoll 子路由色（`GroupId::PianoRoll::child_color`）。
const PIANO_ROLL_CHILD: Color32 = Color32::from_rgb(0xA6, 0x59, 0x59);

/// 左侧栏：路由栏 + 音轨面板。
pub fn show(ui: &mut egui::Ui, state: &mut ChromeState) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, theme::PANEL_BG);
    let route = egui::Rect::from_min_size(rect.min, Vec2::new(layout::ROUTE_BAR_W, rect.height()));
    ui.painter().rect_filled(route, 0, theme::ROUTE_BAR_BG);

    StripBuilder::new(ui)
        .size(Size::exact(layout::ROUTE_BAR_W))
        .size(Size::remainder())
        .horizontal(|mut strip| {
            strip.cell(|ui| route_bar(ui, state));
            strip.cell(|ui| track_panel(ui, state));
        });
}

fn route_bar(ui: &mut egui::Ui, state: &mut ChromeState) {
    // 与 `sidebar::core::types::ROUTES` 同序（Space 用 add_space 表达）。
    const TOP: &[RouteEntry] = &[
        RouteEntry {
            icon: Icon::Keys,
            active_color: Some(PIANO_ROLL_PARENT),
        },
        RouteEntry {
            icon: Icon::FolderTree,
            active_color: Some(PIANO_ROLL_CHILD),
        },
        RouteEntry {
            icon: Icon::WaveForm,
            active_color: None,
        },
        RouteEntry {
            icon: Icon::Arrangement,
            active_color: None,
        },
        RouteEntry {
            icon: Icon::PlayCircle,
            active_color: None,
        },
        RouteEntry {
            icon: Icon::Download,
            active_color: None,
        },
        RouteEntry {
            icon: Icon::VideoCamera,
            active_color: None,
        },
        RouteEntry {
            icon: Icon::MusicNote,
            active_color: None,
        },
    ];

    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for entry in TOP {
            widgets::route_button(ui, &mut state.icons, entry.icon, entry.active_color, false);
        }
        ui.add_space((ui.available_height() - 3.0 * layout::ROUTE_BAR_W - 8.0).max(0.0));
        for (index, icon) in [Icon::RollBarHorizontal, Icon::RollBarVertical, Icon::Mixer]
            .into_iter()
            .enumerate()
        {
            // 水平卷帘为当前布局，点亮；其余熄灭。
            let color = (index == 0).then_some(PIANO_ROLL_CHILD);
            widgets::route_button(ui, &mut state.icons, icon, color, false);
        }
    });
}

fn track_panel(ui: &mut egui::Ui, state: &mut ChromeState) {
    let tracks = state.tracks.clone();
    let mut selected = state
        .tracks
        .iter()
        .position(|track| track.selected)
        .unwrap_or(0);

    let panel_rect = ui.max_rect();
    // 生产 iced：面板内边距 8，列表标题 12px，行高 34 + 行间距 2。
    let inset = 8.0;
    let left = panel_rect.left() + inset;
    let right = panel_rect.right() - inset;
    let list_top = panel_rect.top() + layout::TRACK_LIST_HEADER_H;
    let row_pitch = layout::TRACK_ROW_H + 2.0;
    let list_height = row_pitch * tracks.len() as f32 + layout::TRACK_ROW_H;
    let list_rect = egui::Rect::from_min_size(
        egui::pos2(left, list_top),
        Vec2::new(right - left, list_height),
    );
    ui.painter().rect_filled(list_rect, 0, theme::BASE_BG);
    ui.painter().text(
        egui::pos2(left, panel_rect.top() + 15.0),
        egui::Align2::LEFT_CENTER,
        "音轨列表",
        egui::FontId::proportional(12.0),
        theme::TEXT,
    );

    let mut list_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(egui::Rect::from_min_size(
                list_top_and_left(&list_rect),
                Vec2::new(list_rect.width(), row_pitch * tracks.len() as f32),
            ))
            .layout(Layout::top_down(Align::Min)),
    );

    TableBuilder::new(&mut list_ui)
        .id_salt("lumino-track-list")
        .striped(false)
        .column(Column::remainder())
        .body(|body| {
            body.rows(row_pitch, tracks.len(), |mut row| {
                let index = row.index();
                row.col(|ui| {
                    let response = track_row(ui, &tracks[index], &mut state.icons);
                    if response.clicked() {
                        selected = index;
                    }
                });
            });
        });

    for (index, track) in state.tracks.iter_mut().enumerate() {
        track.selected = index == selected;
    }

    let add_rect = egui::Rect::from_min_size(
        egui::pos2(
            list_rect.left(),
            list_rect.top() + row_pitch * tracks.len() as f32,
        ),
        Vec2::new(list_rect.width(), layout::TRACK_ROW_H),
    );
    let mut add_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(add_rect)
            .layout(Layout::top_down(Align::Min)),
    );
    add_track_row(&mut add_ui, &mut state.icons);
}

/// 列表行区域起点（行内容顶部对齐，行高由 TableBuilder 的 pitch 决定）。
fn list_top_and_left(rect: &egui::Rect) -> egui::Pos2 {
    rect.min
}

fn track_row(
    ui: &mut egui::Ui,
    track: &TrackRow,
    cache: &mut crate::icons::IconCache,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), layout::TRACK_ROW_H),
        Sense::click(),
    );
    if track.selected {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(6), theme::SELECTED_BG);
    }

    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(4.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.spacing_mut().item_spacing.x = 4.0;

    let icon_rect = egui::Rect::from_min_size(
        egui::pos2(child.max_rect().left() + 2.0, rect.center().y - 9.0),
        Vec2::splat(18.0),
    );
    if track.conductor {
        child.painter().image(
            cache
                .texture(child.ctx(), Icon::Clock, 18.0, Icon::Clock.default_tint())
                .id(),
            icon_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    } else {
        child.painter().text(
            egui::pos2(icon_rect.left(), rect.center().y),
            egui::Align2::LEFT_CENTER,
            &track.label,
            egui::FontId::proportional(theme::font::LABEL),
            track.color.unwrap_or(theme::TEXT),
        );
    }
    child.add_space(24.0 + 4.0);

    child.add(
        egui::Label::new(
            egui::RichText::new(&track.name)
                .size(theme::font::LABEL)
                .color(track.color.unwrap_or(theme::TEXT)),
        )
        .truncate(),
    );

    let solo_rect = egui::Rect::from_min_size(
        egui::pos2(rect.right() - 20.0, rect.center().y - 9.0),
        Vec2::new(16.0, 18.0),
    );
    child.painter().text(
        solo_rect.center(),
        egui::Align2::CENTER_CENTER,
        "S",
        egui::FontId::proportional(theme::font::LABEL),
        theme::TEXT,
    );

    response
}

fn add_track_row(ui: &mut egui::Ui, cache: &mut crate::icons::IconCache) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), layout::TRACK_ROW_H),
        Sense::click(),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(4.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.spacing_mut().item_spacing.x = 4.0;
    child.add_space(2.0);
    child.add(cache.default_image(child.ctx(), Icon::Plus, 18.0));
    child.add_space(2.0);
    child.add(egui::Label::new(
        egui::RichText::new("添加音轨")
            .size(theme::font::LABEL)
            .color(theme::TEXT),
    ));
}

/// 右侧栏：图标列（面板默认收起，无激活指示条）。
pub fn show_right(ui: &mut egui::Ui, state: &mut ChromeState) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, theme::ROUTE_BAR_BG);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for icon in [
            Icon::ImageToMidi,
            Icon::MaterialLibrary,
            Icon::PianoWaterfall,
        ] {
            widgets::route_button(ui, &mut state.icons, icon, None, true);
        }
    });
}
