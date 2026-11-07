//! 左侧音轨栏 / 右侧图标栏。
//!
//! 路由栏条目与生产 `ROUTES` 一一对应：分组父按钮激活时点亮 `parent_color`，
//! 子路由激活时点亮 `child_color`，其余条目只显示图标。

mod track_list;

use egui::{Color32, Vec2};
use egui_extras::{Size, StripBuilder};

use super::ChromeState;
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
            strip.cell(|ui| track_list::show(ui, state));
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
