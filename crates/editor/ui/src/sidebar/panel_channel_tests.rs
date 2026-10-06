//! 侧边栏单测 —— 音轨通道选择
//!
//! 自 `panel_tests` 拆出（避免单测模块超 400 行），覆盖标签/重排/pending/回滚载体。

use crate::sidebar::*;

/// 打开通道选择器必须关闭面板空白菜单（防选完通道后残留「找回删除音轨」）
#[test]
fn test_channel_picker_opened_closes_panel_menu() {
    let mut sidebar = Sidebar::new();
    sidebar.panel_context_menu.is_open = true;

    sidebar.update(Event::TrackChannelPickerOpened(1));

    assert!(
        !sidebar.panel_context_menu.is_open,
        "选择器与面板空白菜单必须互斥（防残留浮层）"
    );
}

/// 通道选择会更新标签、按 (port, channel, id) 重排并写入待应用变更（含旧通道）
#[test]
fn test_track_channel_selected_updates_label_and_pending() {
    let mut sidebar = Sidebar::new();
    let track_id = sidebar.tracks[1].id;

    sidebar.update(Event::TrackChannelPickerOpened(track_id));
    assert_eq!(sidebar.channel_picking_track, Some(track_id));

    sidebar.update(Event::TrackChannelSelected(track_id, 3));
    assert!(sidebar.channel_picking_track.is_none());

    let track = sidebar
        .tracks
        .iter()
        .find(|t| t.id == track_id)
        .expect("应找到目标音轨");
    assert_eq!(track.channel, 3);
    assert_eq!(
        track.display_label, "A04",
        "通道号应随通道更新（显示 1 基）"
    );
    assert_eq!(
        sidebar.take_pending_track_channel_change(),
        Some((track_id, 0, 3)),
        "pending 应携带 (id, 旧通道, 新通道) 供数据改写与失败回滚"
    );
    assert_eq!(
        sidebar.take_pending_track_channel_change(),
        None,
        "取出后应清空"
    );
}

/// 点中当前通道是无操作：不置脏、不产生 pending、选择器正常关闭
#[test]
fn test_track_channel_selected_same_channel_is_noop() {
    let mut sidebar = Sidebar::new();
    let track_id = sidebar.tracks[1].id;
    assert_eq!(sidebar.tracks[1].channel, 0, "默认通道应为 0");

    sidebar.update(Event::TrackChannelPickerOpened(track_id));
    sidebar.update(Event::TrackChannelSelected(track_id, 0));

    assert!(sidebar.channel_picking_track.is_none(), "选择器仍应关闭");
    assert_eq!(
        sidebar.take_pending_track_channel_change(),
        None,
        "通道未变化不得产生待应用变更（防误置工程脏）"
    );
}

/// 通道越界输入夹紧到标准上限（内部 15 / 显示 16）
#[test]
fn test_track_channel_selected_clamps_to_channel_count() {
    let mut sidebar = Sidebar::new();
    let track_id = sidebar.tracks[1].id;

    sidebar.update(Event::TrackChannelSelected(track_id, 99));

    let track = sidebar
        .tracks
        .iter()
        .find(|t| t.id == track_id)
        .expect("应找到目标音轨");
    assert_eq!(track.channel, 15, "超上限通道应夹紧到 15（显示 16）");
    assert_eq!(track.display_label, "A16");
    assert_eq!(
        sidebar.take_pending_track_channel_change(),
        Some((track_id, 0, 15))
    );
}

/// 指挥轨不参与通道编辑
#[test]
fn test_track_channel_selected_ignores_conductor() {
    let mut sidebar = Sidebar::new();
    let conductor_id = sidebar.tracks[0].id;

    sidebar.update(Event::TrackChannelSelected(conductor_id, 3));

    assert_eq!(sidebar.tracks[0].channel, 0, "指挥轨通道不应改变");
    assert_eq!(sidebar.take_pending_track_channel_change(), None);
}

/// 通道编辑后按 (port, channel, id) 重排
#[test]
fn test_track_channel_selected_resorts_tracks() {
    let mut sidebar = Sidebar::new();
    sidebar.update_tracks_from_midi(&[
        (0, Some("Conductor".to_string()), 0, 0, 0),
        (1, Some("C5".to_string()), 0, 5, 0),
        (2, Some("C1".to_string()), 0, 1, 0),
    ]);
    // 初始排序：channel0 id0 → channel1 id2 → channel5 id1
    assert_eq!(
        sidebar.tracks.iter().map(|t| t.id).collect::<Vec<_>>(),
        vec![0, 2, 1]
    );

    sidebar.update(Event::TrackChannelSelected(1, 0));

    // id1 从 channel5 改到 channel0 后应排到 id0 之后
    assert_eq!(
        sidebar.tracks.iter().map(|t| t.id).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
}
