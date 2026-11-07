//! `egui_chrome` 的模型装配单测：状态映射与瞬态让位。

use super::*;
use lumino_extras::i18n::Language;
use lumino_ui_egui::chrome::{DialogModel, DialogPalette, SettingsPalette};

fn sidebar_model(sidebar: &Sidebar) -> Option<SidebarModel> {
    sidebar.egui_model(Language::ZhCn, false)
}

#[test]
fn sidebar_model_describes_default_panel() {
    let model = sidebar_model(&Sidebar::new()).expect("默认状态应可接管");
    assert!(model.panel_visible);
    assert_eq!(model.panel, SidebarPanel::Tracks);
    assert!(model.file_active);
    assert_eq!(model.tracks.len(), 2);
    assert!(model.tracks[0].conductor);
    assert!(model.tracks[0].selected);
    assert_eq!(model.tracks[1].label, "A01");
    assert_eq!(model.tracks[1].name, "Setup");
    assert!((model.total_width() - 248.0).abs() < f32::EPSILON);
    assert!(!model.track_list_title.is_empty());
    assert!(!model.add_track_label.is_empty());
}

#[test]
fn sidebar_model_hides_panel_without_disabling_takeover() {
    let mut sidebar = Sidebar::new();
    sidebar.panel_visible = false;
    let model = sidebar_model(&sidebar).expect("隐藏面板不是瞬态 UI");
    assert!(!model.panel_visible);
    assert!(!model.file_active);
    assert!((model.total_width() - 48.0).abs() < f32::EPSILON);
}

#[test]
fn sidebar_model_defers_to_iced_while_transient() {
    let mut renaming = Sidebar::new();
    renaming.renaming_track = Some((1, "草稿".to_owned()));
    assert!(sidebar_model(&renaming).is_none(), "重命名中不得接管");

    let mut menu = Sidebar::new();
    menu.track_context_menu.target_track_id = Some(1);
    assert!(sidebar_model(&menu).is_none(), "音轨菜单打开时不得接管");

    let mut menu = Sidebar::new();
    menu.panel_context_menu.is_open = true;
    assert!(sidebar_model(&menu).is_none(), "面板菜单打开时不得接管");

    let mut picking = Sidebar::new();
    picking.color_picking_track = Some(1);
    assert!(sidebar_model(&picking).is_none(), "取色中不得接管");

    let mut picking = Sidebar::new();
    picking.port_picking_track = Some(1);
    assert!(sidebar_model(&picking).is_none(), "端口选择中不得接管");

    let mut picking = Sidebar::new();
    picking.channel_picking_track = Some(1);
    assert!(sidebar_model(&picking).is_none(), "通道选择中不得接管");

    let mut resizing = Sidebar::new();
    resizing.is_resizing = true;
    assert!(sidebar_model(&resizing).is_none(), "拖拽宽度时不得接管");
}

#[test]
fn sidebar_model_allows_candidate_reorder() {
    // 仅按下（候选）未激活时，插入指示线尚未出现，可以继续由 egui 接管。
    let mut sidebar = Sidebar::new();
    sidebar.track_reorder = Some(crate::sidebar::TrackReorderState {
        track_id: 1,
        started_at: std::time::Instant::now(),
        press_pos: iced_core::Point::new(0.0, 0.0),
        active: false,
        hover_index: None,
    });
    assert!(sidebar_model(&sidebar).is_some());

    if let Some(state) = sidebar.track_reorder.as_mut() {
        state.active = true;
        state.hover_index = Some(1);
    }
    assert!(sidebar_model(&sidebar).is_none(), "拖拽激活时不得接管");
}

#[test]
fn right_sidebar_model_follows_visibility_and_active_panel() {
    let mut root = Root::new(&lumino_core::storage::config::UiConfig::default());

    let model = root.egui_chrome_model();
    let right = model.right_sidebar.expect("钢琴卷帘视图应接管右侧栏");
    assert!(!right.panel_visible);
    assert!(right.active.is_none(), "面板收起时无点亮图标");
    assert!(
        (right.top_inset - 102.0).abs() < f32::EPSILON,
        "图标列顶边 = 标题栏 30 + 工具栏 72"
    );
    assert!((right.total_width() - 48.0).abs() < f32::EPSILON);

    // 面板打开：点亮当前面板，总宽 = 48 + panel_width。
    root.right_sidebar.panel_visible = true;
    root.right_sidebar.active_panel = crate::right_sidebar::RightSidebarPanel::Materials;
    let right = root.egui_chrome_model().right_sidebar.expect("仍应接管");
    assert_eq!(right.active, Some(RightSidebarPanelKind::Materials));
    assert!((right.total_width() - 248.0).abs() < f32::EPSILON);

    // 卷帘关闭后 iced 不渲染右侧栏，叠加层同步退出。
    root.sidebar.piano_roll_visible = false;
    assert!(root.egui_chrome_model().right_sidebar.is_none());
}

#[test]
fn piano_roll_model_tracks_editor_view_state() {
    let mut root = Root::new(&lumino_core::storage::config::UiConfig::default());
    // 首帧布局前画布尺寸为 0（生产里由 iced 布局回填）。
    root.editor.editor_state.canvas.size_x = 1280.0;
    root.editor.editor_state.canvas.size_y = 858.0;
    let piano = root.egui_piano_roll_model().expect("默认工程应可接管");

    // 内缩：左 48+200、上 30+72、右 48（右侧栏图标列）、下 20（状态栏）。
    assert!((piano.left_inset - 248.0).abs() < f32::EPSILON);
    assert!((piano.top_inset - 102.0).abs() < f32::EPSILON);
    assert!((piano.right_inset - 48.0).abs() < f32::EPSILON);
    assert!((piano.bottom_inset - 20.0).abs() < f32::EPSILON);
    assert!((piano.keyboard_width - 120.0).abs() < f32::EPSILON);
    assert!((piano.ruler_height - 24.0).abs() < f32::EPSILON);
    assert_eq!(piano.visible_keys, 128);
    assert!((piano.playhead_offset - 0.0).abs() < f32::EPSILON);

    // 视口 → 卷帘区域换算。
    let area = piano.area(egui::Rect::from_min_size(
        egui::pos2(0.0, 0.0),
        egui::vec2(1600.0, 1000.0),
    ));
    assert!((area.left() - 248.0).abs() < f32::EPSILON);
    assert!((area.top() - 102.0).abs() < f32::EPSILON);
    assert!((area.right() - 1552.0).abs() < f32::EPSILON);
    assert!((area.bottom() - 980.0).abs() < f32::EPSILON);

    // 缩放/滚动直接来自 Editor 视图状态。
    root.editor.editor_state.view.zoom_x = 0.05;
    root.editor.editor_state.view.scroll_x = 3072.0;
    let piano = root.egui_piano_roll_model().expect("改缩放不应退出接管");
    assert!((piano.zoom_x - 0.05).abs() < f32::EPSILON);
    assert!((piano.scroll_x - 3072.0).abs() < f32::EPSILON);
    assert!((piano.bar_pixels() - 384.0).abs() < 0.01);
}

#[test]
fn piano_roll_model_defers_to_iced_for_dynamic_overlays() {
    // 洋葱皮：琴键带彩色覆盖层。
    let mut onion = Root::new(&lumino_core::storage::config::UiConfig::default());
    onion.toolbar.onion_skin_enabled = true;
    assert!(onion.egui_piano_roll_model().is_none());

    // 纵向卷帘：键盘在底部，布局不同。
    let mut vertical = Root::new(&lumino_core::storage::config::UiConfig::default());
    vertical.sidebar.roll_bar_active = Some(crate::sidebar::RollBarButton::Vertical);
    assert!(vertical.egui_piano_roll_model().is_none());

    // 协作远程光标可落在键盘/标尺上。
    let mut remote = Root::new(&lumino_core::storage::config::UiConfig::default());
    remote.editor.remote_cursors.insert(
        "peer".to_owned(),
        (
            iced_core::Point::new(10.0, 10.0),
            "#FF0000".to_owned(),
            "peer".to_owned(),
        ),
    );
    assert!(remote.egui_piano_roll_model().is_none());

    // 工程中途改拍号：标尺按拍号分段。
    let mut meter = Root::new(&lumino_core::storage::config::UiConfig::default());
    meter.editor.editor_state.data.time_signatures = vec![(0, 4, 4), (7680, 3, 4)];
    assert!(meter.egui_piano_roll_model().is_none());
}

#[test]
fn piano_roll_overlay_is_off_by_default() {
    // 生产里卷帘骨架由 `lumino-gfx` 绘制；egui 骨架默认不接管，避免把
    // 用户当前的网格配色盖成 iced 深色键盘。
    let root = Root::new(&lumino_core::storage::config::UiConfig::default());
    assert!(root.egui_chrome_model().piano_roll.is_none());
}

#[test]
fn dialog_windows_do_not_receive_main_chrome() {
    let config = lumino_core::storage::config::UiConfig::default();
    let root = Root::new_settings_dialog(&config.theme, &config);
    let model = root.egui_chrome_model();
    assert!(model.titlebar.is_none(), "对话框不得画主窗口标题栏");
    assert!(model.toolbar.is_none(), "对话框不得画工具栏");
    assert!(model.sidebar.is_none(), "对话框不得画左侧栏");
    assert!(model.right_sidebar.is_none(), "对话框不得画右侧栏");
    assert!(model.piano_roll.is_none(), "对话框不得画卷帘骨架");
    assert!(model.statusbar.fps.is_none(), "对话框不得画状态栏");
    assert!(
        matches!(model.dialog, Some(DialogModel::Settings(_))),
        "设置对话框已整窗接管（调色板来自生产主题）"
    );
}

#[test]
fn save_confirm_dialog_model_carries_localized_text() {
    // 深色主题下才整窗接管（本层只有深色调色板）。
    let theme = lumino_ui_core::Theme::TokyoNightStorm.to_string();
    let root = Root::new_dialog(&theme, crate::state::root_state::DialogType::SaveConfirm);
    let model = root.egui_chrome_model();
    assert!(model.titlebar.is_none(), "整窗模型不再叠加主窗口标题栏");
    let Some(DialogModel::SaveConfirm(save)) = model.dialog else {
        panic!("保存确认对话框必须整窗接管");
    };
    assert!(save.titlebar, "对话框默认使用自制标题栏");
    assert_eq!(
        save.palette,
        DialogPalette::DARK,
        "深色主题映射应与基线一致"
    );
    assert!(!save.message.is_empty());
    assert!(!save.save.is_empty());
    assert!(!save.close.is_empty());
    assert!(!save.cancel.is_empty());
}

#[test]
fn light_theme_dialogs_use_injected_palette() {
    let light = lumino_ui_core::Theme::Light.to_string();
    let root = Root::new_dialog(&light, crate::state::root_state::DialogType::SaveConfirm);
    let model = root.egui_chrome_model();
    let Some(DialogModel::SaveConfirm(save)) = model.dialog else {
        panic!("浅色主题的保存确认对话框也应整窗接管（调色板来自 iced）");
    };
    assert_ne!(
        save.palette.background,
        DialogPalette::DARK.background,
        "浅色调色板必须来自生产主题，而不是深色常量"
    );

    // 设置页同样注入调色板：浅色主题也整窗接管。
    let settings =
        Root::new_settings_dialog(&light, &lumino_core::storage::config::UiConfig::default());
    let model = settings.egui_chrome_model();
    let Some(DialogModel::Settings(values)) = model.dialog else {
        panic!("浅色主题的设置对话框也应整窗接管");
    };
    assert_ne!(
        values.palette.background,
        SettingsPalette::DARK.background,
        "浅色设置调色板必须来自生产主题"
    );
}

#[test]
fn settings_dialog_model_carries_general_page_values() {
    let theme = lumino_ui_core::Theme::TokyoNightStorm.to_string();
    let config = lumino_core::storage::config::UiConfig::default();
    let mut root = Root::new_settings_dialog(&theme, &config);

    let model = root.egui_chrome_model();
    let Some(DialogModel::Settings(values)) = model.dialog else {
        panic!("设置对话框「常规」页应整窗接管");
    };
    assert_eq!(values.selected, 0);
    assert_eq!(values.labels.menu.len(), 10);
    assert_eq!(values.labels.title, "常规");
    assert_eq!(values.labels.confirm, "确认");
    assert_eq!(values.labels.eraser_options.len(), 2);
    assert_eq!(values.labels.track_add_options.len(), 2);
    assert!(!values.eraser.is_empty(), "橡皮擦行为应来自真实设置");
    assert!(!values.track_add.is_empty(), "添加音轨行为应来自真实设置");
    assert_eq!(
        values.log_retention,
        root.settings.logging.log_retention_count.to_string(),
        "日志保留份数必须来自 logging（不是历史条数）"
    );

    // 其它页面尚未建模：整块交回 iced。
    root.settings.selected_menu_index = 1;
    assert!(root.egui_chrome_model().dialog.is_none());
}

#[test]
fn settings_dialog_labels_follow_language() {
    let theme = lumino_ui_core::Theme::TokyoNightStorm.to_string();
    let config = lumino_core::storage::config::UiConfig {
        language: lumino_extras::i18n::Language::EnUs,
        ..lumino_core::storage::config::UiConfig::default()
    };
    let root = Root::new_settings_dialog(&theme, &config);

    let model = root.egui_chrome_model();
    let Some(DialogModel::Settings(values)) = model.dialog else {
        panic!("英文界面下设置对话框也应整窗接管");
    };
    let t = lumino_extras::i18n::settings_translations(lumino_extras::i18n::Language::EnUs);
    assert_eq!(values.labels.title, t.general_title);
    assert_eq!(values.labels.confirm, t.confirm);
    assert_eq!(values.labels.menu.len(), 10);
    assert_eq!(values.labels.menu[0], t.general);
    assert_eq!(values.labels.menu[8], t.compatibility);
    assert_eq!(values.labels.menu[9], t.about);
    assert_eq!(values.labels.eraser_label, t.eraser_behavior);
    assert_eq!(values.labels.log_label, t.log_retention_count);
    assert_eq!(values.eraser, "Default (Shift+drag)");
    assert_eq!(values.track_add, "Auto-switch to new track");
}

#[test]
fn settings_dialog_takes_over_at_any_window_size() {
    let theme = lumino_ui_core::Theme::TokyoNightStorm.to_string();
    let config = lumino_core::storage::config::UiConfig::default();
    let root = Root::new_settings_dialog(&theme, &config);

    // 默认对话框 720×540 逻辑点：菜单放不下，但菜单现在能滚（滚轮事件已同步
    // 给 egui），因此仍整窗接管，不再把「云管理/兼容性/关于」交回 iced。
    assert!(
        matches!(
            root.egui_chrome_model().dialog,
            Some(DialogModel::Settings(_))
        ),
        "菜单需要滚动时也必须整窗接管（由 egui 自己滚）"
    );
}

#[test]
fn progress_and_warning_windows_are_not_covered() {
    let config = lumino_core::storage::config::UiConfig::default();

    let progress = Root::new_progress(&config.theme, &config);
    let model = progress.egui_chrome_model();
    assert!(model.titlebar.is_none() && model.statusbar.fps.is_none());
    assert!(model.dialog.is_none());

    let warning = Root::new_device_warning(&config.theme, &config, "测试".to_owned());
    let model = warning.egui_chrome_model();
    assert!(model.titlebar.is_none() && model.statusbar.fps.is_none());
    assert!(model.dialog.is_none());
}

#[test]
fn iced_popups_disable_the_overlay() {
    // 菜单栏下拉：打开时必须整块让位，否则菜单会被不透明 band 盖住。
    let mut menu = Root::new(&lumino_core::storage::config::UiConfig::default());
    menu.state.is_menu_open = true;
    let model = menu.egui_chrome_model();
    assert!(model.titlebar.is_none(), "菜单打开时不得画标题栏 band");
    assert!(model.sidebar.is_none(), "菜单打开时不得画左侧栏 band");
    assert!(model.toolbar.is_none(), "菜单打开时不得画工具栏 band");
    assert!(model.right_sidebar.is_none());
    assert!(model.statusbar.fps.is_none());

    // 工具栏下拉（吸附 / 洋葱皮 / 溢出 / 工具面板 …）同理。
    let mut toolbar = Root::new(&lumino_core::storage::config::UiConfig::default());
    toolbar.toolbar.snap_picker_open = true;
    assert!(toolbar.egui_chrome_model().toolbar.is_none());

    let mut toolbar = Root::new(&lumino_core::storage::config::UiConfig::default());
    toolbar.toolbar.tool_panel_open = true;
    assert!(toolbar.egui_chrome_model().sidebar.is_none());

    // 标题栏闩锁（菜单栏下拉不可探测）：按下后让位，下一次别处按下恢复。
    let mut latch = Root::new(&lumino_core::storage::config::UiConfig::default());
    assert!(latch.set_egui_titlebar_latch(true), "闩锁状态应发生变化");
    assert!(latch.egui_chrome_model().titlebar.is_none());
    assert!(!latch.set_egui_titlebar_latch(true), "重复置位不算变化");
    assert!(latch.set_egui_titlebar_latch(false));
    assert!(latch.egui_chrome_model().titlebar.is_some());
}
