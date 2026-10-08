//! 侧边栏单测 —— 音轨颜色选择器互斥
//!
//! 对齐 `panel_port_tests` / `panel_channel_tests`：打开颜色选择器必须
//! 关闭其他悬浮层（面板空白菜单 / 端口选择器 / 通道选择器 / 重命名态），
//! 防止选完颜色后残留浮层或事件落空。

use crate::sidebar::*;

/// 打开颜色选择器必须关闭面板空白菜单（防选完颜色后残留「找回删除音轨」浮层）
#[test]
fn test_color_picker_opened_closes_panel_menu() {
    let mut sidebar = Sidebar::new();
    sidebar.panel_context_menu.is_open = true;

    sidebar.update(Event::TrackColorPickerOpened(1));

    assert!(
        !sidebar.panel_context_menu.is_open,
        "颜色选择器与面板空白菜单必须互斥（防残留浮层）"
    );
}

/// 打开颜色选择器必须取代端口/通道选择器并关闭重命名态（互斥模型）
#[test]
fn test_color_picker_opened_closes_other_overlays() {
    let mut sidebar = Sidebar::new();
    let track_id = sidebar.tracks[1].id;

    sidebar.update(Event::TrackPortPickerOpened(track_id));
    assert_eq!(sidebar.port_picking_track, Some(track_id));

    sidebar.update(Event::TrackColorPickerOpened(track_id));

    assert_eq!(sidebar.color_picking_track, Some(track_id));
    assert_eq!(
        sidebar.port_picking_track, None,
        "端口选择器必须被颜色选择器取代"
    );
    assert_eq!(sidebar.channel_picking_track, None);
    assert_eq!(sidebar.renaming_track, None, "重命名态必须被颜色选择器关闭");
}

/// 反向互斥：打开端口选择器必须取代颜色选择器（对齐既有 port 行为）
#[test]
fn test_port_picker_opened_closes_color_picker() {
    let mut sidebar = Sidebar::new();
    let track_id = sidebar.tracks[1].id;

    sidebar.update(Event::TrackColorPickerOpened(track_id));
    assert_eq!(sidebar.color_picking_track, Some(track_id));

    sidebar.update(Event::TrackPortPickerOpened(track_id));

    assert_eq!(sidebar.port_picking_track, Some(track_id));
    assert_eq!(
        sidebar.color_picking_track, None,
        "颜色选择器必须被端口选择器取代"
    );
}

/// 颜色选择完成：写回颜色并关闭选择器
#[test]
fn test_color_selected_applies_and_closes() {
    use iced_core::Color;

    let mut sidebar = Sidebar::new();
    let track_id = sidebar.tracks[1].id;
    let color = Color::from_rgb(0.5, 0.25, 0.75);

    sidebar.update(Event::TrackColorPickerOpened(track_id));
    sidebar.update(Event::TrackColorSelected(track_id, color));

    let track = sidebar
        .tracks
        .iter()
        .find(|t| t.id == track_id)
        .expect("应找到目标音轨");
    assert_eq!(track.color, Some(color));
    assert_eq!(sidebar.color_picking_track, None, "选择完成应关闭选择器");
}
