//! 侧边栏单测 —— 音轨端口选择
//!
//! 自 `panel_tests` 拆出（避免单测模块超 400 行），覆盖标签/重排/pending/回滚载体。

use crate::sidebar::*;

/// 打开端口选择器必须关闭面板空白菜单（防选完端口后残留「找回删除音轨」）
#[test]
fn test_port_picker_opened_closes_panel_menu() {
    let mut sidebar = Sidebar::new();
    sidebar.panel_context_menu.is_open = true;

    sidebar.update(Event::TrackPortPickerOpened(1));

    assert!(
        !sidebar.panel_context_menu.is_open,
        "选择器与面板空白菜单必须互斥（防残留浮层）"
    );
}

/// 端口选择会更新标签、按 (port, channel, id) 重排并写入待应用变更（含旧端口）
#[test]
fn test_track_port_selected_updates_label_and_pending() {
    let mut sidebar = Sidebar::new();
    let track_id = sidebar.tracks[1].id;

    sidebar.update(Event::TrackPortPickerOpened(track_id));
    assert_eq!(sidebar.port_picking_track, Some(track_id));

    sidebar.update(Event::TrackPortSelected(track_id, 3));
    assert!(sidebar.port_picking_track.is_none());

    let track = sidebar
        .tracks
        .iter()
        .find(|t| t.id == track_id)
        .expect("应找到目标音轨");
    assert_eq!(track.port, 3);
    assert_eq!(track.display_label, "D01", "端口字母应随端口更新");
    assert_eq!(
        sidebar.take_pending_track_port_change(),
        Some((track_id, 0, 3)),
        "pending 应携带 (id, 旧端口, 新端口) 供失败回滚"
    );
    assert_eq!(
        sidebar.take_pending_track_port_change(),
        None,
        "取出后应清空"
    );
}

/// 点中当前端口是无操作：不置脏、不产生 pending、选择器正常关闭
#[test]
fn test_track_port_selected_same_port_is_noop() {
    let mut sidebar = Sidebar::new();
    let track_id = sidebar.tracks[1].id;
    assert_eq!(sidebar.tracks[1].port, 0, "默认端口应为 0");

    sidebar.update(Event::TrackPortPickerOpened(track_id));
    sidebar.update(Event::TrackPortSelected(track_id, 0));

    assert!(sidebar.port_picking_track.is_none(), "选择器仍应关闭");
    assert_eq!(
        sidebar.take_pending_track_port_change(),
        None,
        "端口未变化不得产生待应用变更（防误置工程脏）"
    );
}

/// 端口编辑越界输入夹紧到产品上限（内部 15 / 显示 16）
#[test]
fn test_track_port_selected_clamps_to_product_limit() {
    let mut sidebar = Sidebar::new();
    let track_id = sidebar.tracks[1].id;

    sidebar.update(Event::TrackPortSelected(track_id, 99));

    let track = sidebar
        .tracks
        .iter()
        .find(|t| t.id == track_id)
        .expect("应找到目标音轨");
    assert_eq!(track.port, 15, "超上限端口应夹紧到 15（显示 16）");
    assert_eq!(track.display_label, "P01");
    assert_eq!(
        sidebar.take_pending_track_port_change(),
        Some((track_id, 0, 15))
    );
}

/// 指挥轨不参与端口编辑
#[test]
fn test_track_port_selected_ignores_conductor() {
    let mut sidebar = Sidebar::new();
    let conductor_id = sidebar.tracks[0].id;

    sidebar.update(Event::TrackPortSelected(conductor_id, 3));

    assert_eq!(sidebar.tracks[0].port, 0, "指挥轨端口不应改变");
    assert_eq!(sidebar.take_pending_track_port_change(), None);
}

/// 端口编辑后按 (port, channel, id) 重排
#[test]
fn test_track_port_selected_resorts_tracks() {
    let mut sidebar = Sidebar::new();
    sidebar.update_tracks_from_midi(&[
        (0, Some("Conductor".to_string()), 0, 0, 0),
        (1, Some("P5".to_string()), 0, 0, 5),
        (2, Some("P1".to_string()), 0, 0, 1),
    ]);
    // 初始排序：port0 id0 → port1 id2 → port5 id1
    assert_eq!(
        sidebar.tracks.iter().map(|t| t.id).collect::<Vec<_>>(),
        vec![0, 2, 1]
    );

    sidebar.update(Event::TrackPortSelected(1, 0));

    // id1 从 port5 改到 port0 后应排到 id0 之后
    assert_eq!(
        sidebar.tracks.iter().map(|t| t.id).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
}
