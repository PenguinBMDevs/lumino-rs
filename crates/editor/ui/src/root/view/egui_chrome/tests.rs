//! `egui_chrome` 的模型装配单测：状态映射与瞬态让位。

use super::*;
use crate::toolbar::Popover;
use lumino_core::SnapSpec;
use lumino_core::i18n::Language;
use lumino_ui_core::onion_skin_mode::OnionSkinMode;
use lumino_ui_egui::chrome::{
    DialogModel, DialogPalette, MenuEntry, RightSidebarPanelKind, SettingsPalette, SidebarModel,
    SidebarPanel, SidebarRouteIcon, ToolbarAction,
};
use lumino_ui_egui::theme::Palette;

use crate::Message;
use crate::toolbar::Event as ToolbarEvent;

fn sidebar_model(sidebar: &Sidebar) -> Option<SidebarModel> {
    sidebar.egui_model(Language::ZhCn, false, false)
}

#[test]
fn sidebar_buttons_carry_explanatory_tips() {
    let model = sidebar_model(&Sidebar::new()).expect("默认状态应可接管");
    assert!(!model.route_items.is_empty(), "默认应有路由条目");
    for item in &model.route_items {
        assert!(
            !item.tip.name.is_empty(),
            "路由按钮 {:?} 缺少悬停名称",
            item.icon
        );
        assert!(
            !item.tip.description.is_empty(),
            "路由按钮 {:?} 缺少解释说明",
            item.icon
        );
    }
    for (label, tip) in [
        ("横向卷帘", model.roll_bar_horizontal_tip),
        ("纵向卷帘", model.roll_bar_vertical_tip),
        ("混音台", model.mixer_tip),
        ("独奏", model.solo_tip),
    ] {
        assert!(!tip.name.is_empty(), "{label} 缺少悬停名称");
        assert!(!tip.description.is_empty(), "{label} 缺少解释说明");
    }
}

#[test]
fn sidebar_model_carries_route_items() {
    // 默认：PianoRoll 组激活 → Keys + File + Automation，再 Project / Waterfall /
    // Renderer（渲染组未激活，不含两个导出子按钮）。
    let model = sidebar_model(&Sidebar::new()).expect("默认状态应可接管");
    let icons: Vec<SidebarRouteIcon> = model.route_items.iter().map(|item| item.icon).collect();
    assert_eq!(
        icons,
        vec![
            SidebarRouteIcon::PianoRoll,
            SidebarRouteIcon::File,
            SidebarRouteIcon::Automation,
            SidebarRouteIcon::Project,
            SidebarRouteIcon::Waterfall,
            SidebarRouteIcon::Renderer,
        ],
        "默认路由栏条目"
    );

    // 渲染组激活 → 追加视频 / 音频导出；两个子按钮的点亮跟随可见标志。
    let mut sidebar = Sidebar::new();
    sidebar.active_group = Some(crate::sidebar::GroupId::Renderer);
    sidebar.video_export_visible = true;
    let model = sidebar_model(&sidebar).expect("渲染组激活不是瞬态 UI");
    let tail: Vec<SidebarRouteIcon> = model
        .route_items
        .iter()
        .skip(model.route_items.len() - 2)
        .map(|item| item.icon)
        .collect();
    assert_eq!(
        tail,
        vec![SidebarRouteIcon::VideoExport, SidebarRouteIcon::AudioExport],
        "渲染组子按钮顺序"
    );
    assert!(
        model
            .route_items
            .iter()
            .any(|item| item.icon == SidebarRouteIcon::VideoExport && item.active),
        "视频导出可见时应点亮"
    );
    assert!(
        !model
            .route_items
            .iter()
            .any(|item| item.icon == SidebarRouteIcon::AudioExport && item.active),
        "音频导出不可见时不应点亮"
    );
}

#[test]
fn sidebar_model_lights_waterfall_button_in_player_mode() {
    // 播放器模式：黄色组父按钮必须点亮（回归：生产装配曾把 active 写死为 false）。
    let model = Sidebar::new()
        .egui_model(Language::ZhCn, false, true)
        .expect("播放器模式应可接管");
    assert!(model.waterfall, "模型应标记播放器模式");
    assert!(
        model
            .route_items
            .iter()
            .any(|item| item.icon == SidebarRouteIcon::Waterfall && item.active),
        "播放器组按钮应点亮"
    );

    // 非播放器模式：同一按钮不得点亮。
    let model = sidebar_model(&Sidebar::new()).expect("默认状态应可接管");
    assert!(
        model
            .route_items
            .iter()
            .any(|item| item.icon == SidebarRouteIcon::Waterfall && !item.active),
        "编辑模式下播放器组按钮不应点亮"
    );
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
fn sidebar_model_defers_to_legacy_while_transient() {
    let mut renaming = Sidebar::new();
    renaming.renaming_track = Some((1, "草稿".to_owned()));
    let model = sidebar_model(&renaming).expect("重命名由 egui 绘制输入框，仍须接管");
    assert!(
        model
            .tracks
            .iter()
            .any(|track| track.renaming && track.id == 1 && track.rename_buffer == "草稿")
    );

    let mut menu = Sidebar::new();
    menu.track_context_menu.target_track_id = Some(1);
    let model = sidebar_model(&menu).expect("音轨菜单由 egui 绘制，仍须接管");
    assert_eq!(
        model.context_menu.as_ref().map(|menu| menu.track_id),
        Some(1)
    );

    let mut menu = Sidebar::new();
    menu.panel_context_menu.is_open = true;
    menu.panel_context_menu.mouse_pos = Some((120.0, 140.0));
    let model = sidebar_model(&menu).expect("面板菜单由 egui 绘制，仍须接管");
    assert_eq!(
        model.panel_menu.as_ref().map(|menu| menu.pos),
        Some(egui::pos2(120.0, 140.0))
    );

    let mut picking = Sidebar::new();
    picking.color_picking_track = Some(1);
    let model = sidebar_model(&picking).expect("取色器由 egui 绘制，仍须接管");
    assert_eq!(model.color_picker.map(|picker| picker.track_id), Some(1));

    let mut picking = Sidebar::new();
    picking.port_picking_track = Some(1);
    let model = sidebar_model(&picking).expect("端口选择器由 egui 绘制，仍须接管");
    assert_eq!(
        model.numeric_picker,
        Some(lumino_ui_egui::chrome::SidebarNumericPickerModel {
            track_id: 1,
            kind: lumino_ui_egui::chrome::SidebarNumericPickerKind::Port,
            kind_label: "端口",
            current: 0,
            choices: Sidebar::PORT_CHOICES,
        })
    );

    let mut picking = Sidebar::new();
    picking.channel_picking_track = Some(1);
    let model = sidebar_model(&picking).expect("通道选择器由 egui 绘制，仍须接管");
    assert_eq!(
        model.numeric_picker.map(|picker| picker.kind),
        Some(lumino_ui_egui::chrome::SidebarNumericPickerKind::Channel)
    );

    let mut resizing = Sidebar::new();
    resizing.is_resizing = true;
    let model = sidebar_model(&resizing).expect("宽度拖拽由 egui 绘制手柄，仍须接管");
    assert!(model.resizing, "手柄需要拿到拖拽态以切换底色");
}

#[test]
fn sidebar_model_carries_active_reorder() {
    // 候选（未激活）不画指示线；激活后模型带出插入下标供 egui 绘制。
    let mut sidebar = Sidebar::new();
    sidebar.track_reorder = Some(crate::sidebar::TrackReorderState {
        track_id: 1,
        started_at: std::time::Instant::now(),
        press_pos: lumino_ui_core::geom::Point::new(0.0, 0.0),
        active: false,
        hover_index: None,
    });
    let model = sidebar_model(&sidebar).expect("拖拽排序由 egui 绘制");
    assert!(model.reorder.is_none(), "未激活不应画指示线");

    if let Some(state) = sidebar.track_reorder.as_mut() {
        state.active = true;
        state.hover_index = Some(1);
    }
    let model = sidebar_model(&sidebar).expect("拖拽激活后仍由 egui 接管");
    let reorder = model.reorder.expect("激活后应带出排序状态");
    assert_eq!(reorder.track_id, 1);
    assert_eq!(reorder.hover_index, Some(1));
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

    // 卷帘关闭后 上一代 不渲染右侧栏，叠加层同步退出。
    root.sidebar.piano_roll_visible = false;
    assert!(root.egui_chrome_model().right_sidebar.is_none());
}

#[test]
fn piano_roll_model_tracks_editor_view_state() {
    let mut root = Root::new(&lumino_core::storage::config::UiConfig::default());
    // 首帧布局前画布尺寸为 0（生产里由 上一代 布局回填）。
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
fn dynamic_overlays_keep_egui_skeleton() {
    fn root() -> Root {
        let mut root = Root::new(&lumino_core::storage::config::UiConfig::default());
        // 首帧布局前画布尺寸为 0；生产里由布局回填后再出模型。
        root.editor.editor_state.canvas.size_x = 1280.0;
        root.editor.editor_state.canvas.size_y = 858.0;
        root
    }

    // 洋葱皮：键盘列 / 滚动条 / 标尺必须保留（gfx 不补键盘，也不补小节号）。
    let mut onion = root();
    onion.toolbar.onion_skin_enabled = true;
    onion.egui_piano_roll_model().expect("洋葱皮不应让骨架消失");

    // 纵向卷帘：布局不同，但骨架同样必须保留。
    let mut vertical = root();
    vertical.sidebar.roll_bar_active = Some(crate::sidebar::RollBarButton::Vertical);
    let piano = vertical
        .egui_piano_roll_model()
        .expect("纵向卷帘也必须由 egui 绘制键盘");
    assert!(piano.vertical);

    // 协作远程光标画在骨架之上，不应让骨架消失。
    let mut remote = root();
    remote.editor.transient.remote_cursors.insert(
        "peer".to_owned(),
        (
            lumino_ui_core::geom::Point::new(10.0, 10.0),
            "#FF0000".to_owned(),
            "peer".to_owned(),
        ),
    );
    remote
        .egui_piano_roll_model()
        .expect("远程光标不应让骨架消失");

    // 循环区间：标尺里的循环带由叠加层绘制，骨架不得整块让位。
    let mut looping = root();
    if let Some(range) = looping.editor.loop_range.as_mut() {
        range.set_range(0.0, 1920.0);
        range.enable();
    }
    looping
        .egui_piano_roll_model()
        .expect("循环区间不应让骨架消失");

    // 工程中途改拍号：骨架保留，拍号列表随模型下发（egui 标尺按段编号）。
    let mut meter = root();
    meter.editor.editor_state.data.time_signatures = vec![(0, 4, 4), (7680, 3, 4)];
    let piano = meter
        .egui_piano_roll_model()
        .expect("中途变拍不应让骨架消失");
    assert_eq!(
        piano.time_signatures,
        vec![(0, 4, 4), (7680, 3, 4)],
        "拍号分段必须下发给 egui 标尺"
    );
}

#[test]
fn duplicate_time_signatures_do_not_disable_egui_skeleton() {
    // 多轨 MIDI 常见：每条轨各写一个 tick 0 的 4/4。加载器按 EXP-005 口径
    // 全量保留同 tick 拍号（`midi-loader/tests/duplicate_time_signatures.rs`），
    // 去重是显示层的责任。这类重复不构成「中途变拍」，骨架必须继续由 egui
    // 接管——否则键盘列 / 滚动条消失（gfx 网格着色器本就 discard 键盘列，
    // 见 `shaders/infinite_grid.wgsl`），加载 MIDI 后只剩网格与标尺。
    let mut root = Root::new(&lumino_core::storage::config::UiConfig::default());
    root.editor.editor_state.canvas.size_x = 1280.0;
    root.editor.editor_state.canvas.size_y = 858.0;
    root.editor.editor_state.data.time_signatures = vec![(0, 4, 4), (0, 4, 4)];
    let piano = root
        .egui_piano_roll_model()
        .expect("同 tick 重复拍号不应退出接管");
    assert!((piano.bar_pixels() - 768.0).abs() < 0.01, "4/4 小节宽");

    // 同值拍号在工程中途重复（值不变）同样不构成变拍：小节宽不变，标尺连续。
    root.editor.editor_state.data.time_signatures = vec![(0, 4, 4), (7680, 4, 4)];
    assert!(
        root.egui_piano_roll_model().is_some(),
        "同值拍号重复不应退出接管"
    );
}

#[test]
fn piano_roll_overlay_is_off_by_default() {
    // 首帧布局前画布尺寸为 0：模型为空，egui 骨架不接管。
    // 布局回填后键盘列只由 egui 绘制（gfx 网格着色器 discard 键盘列），
    // 模型必须尽快转为 Some——否则加载 MIDI 后骨架会整块消失。
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
        panic!("浅色主题的保存确认对话框也应整窗接管（调色板跟随当前主题）");
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

    // 云管理页也已接管（10/10 设置页全部由 egui 绘制）。
    root.settings.selected_menu_index = 7;
    assert!(root.egui_chrome_model().dialog.is_some());
}

#[test]
fn settings_dialog_labels_follow_language() {
    let theme = lumino_ui_core::Theme::TokyoNightStorm.to_string();
    let config = lumino_core::storage::config::UiConfig {
        language: lumino_core::i18n::Language::EnUs,
        ..lumino_core::storage::config::UiConfig::default()
    };
    let root = Root::new_settings_dialog(&theme, &config);

    let model = root.egui_chrome_model();
    let Some(DialogModel::Settings(values)) = model.dialog else {
        panic!("英文界面下设置对话框也应整窗接管");
    };
    let t = lumino_core::i18n::settings_translations(lumino_core::i18n::Language::EnUs);
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
    // 给 egui），因此仍整窗接管，不再把「云管理/兼容性/关于」交回 上一代。
    assert!(
        matches!(
            root.egui_chrome_model().dialog,
            Some(DialogModel::Settings(_))
        ),
        "菜单需要滚动时也必须整窗接管（由 egui 自己滚）"
    );
}

#[test]
fn progress_and_warning_windows_draw_their_own_content() {
    let config = lumino_core::storage::config::UiConfig::default();

    // 这两类窗口没有 `DialogType`，但该窗口不再 present，内容必须由 egui 绘制，
    // 否则窗口只剩标题栏。同时不能画出主窗口的标题栏 / 状态栏 band。
    let progress = Root::new_progress(&config.theme, &config);
    let model = progress.egui_chrome_model();
    assert!(model.titlebar.is_none() && model.statusbar.fps.is_none());
    assert!(
        matches!(model.dialog, Some(DialogModel::Progress(_))),
        "进度窗必须交出内容模型，实际：{:?}",
        model.dialog
    );

    let warning = Root::new_device_warning(&config.theme, &config, "测试".to_owned());
    let model = warning.egui_chrome_model();
    assert!(model.titlebar.is_none() && model.statusbar.fps.is_none());
    assert!(
        matches!(model.dialog, Some(DialogModel::DeviceWarning(_))),
        "设备警告窗必须交出内容模型，实际：{:?}",
        model.dialog
    );
}

#[test]
fn legacy_popups_no_longer_disable_the_overlay() {
    // 自绘标题栏的菜单已由 egui 接管：`is_menu_open` 不再触发整块让位。
    let mut menu = Root::new(&lumino_core::storage::config::UiConfig::default());
    menu.state.is_menu_open = true;
    let model = menu.egui_chrome_model();
    assert!(
        model.titlebar.is_some() && model.toolbar.is_some(),
        "egui 菜单展开时仍应画外壳"
    );

    // 系统原生标题栏模式：Windows/Linux 的 30px 菜单栏行也由 egui 接管，
    // 上一代 只保留同宽占位，不再整块让位。
    let mut native = Root::new(&lumino_core::storage::config::UiConfig::default());
    native.settings.synth.use_native_titlebar = true;
    native.state.is_menu_open = true;
    let model = native.egui_chrome_model();
    assert!(model.toolbar.is_some(), "原生标题栏模式不再需要菜单让位");
    if !cfg!(target_os = "macos") {
        let titlebar = model.titlebar.expect("原生标题栏的菜单栏应由 egui 绘制");
        assert!(titlebar.menu_only, "原生模式只画菜单栏，无 logo/窗口控制");
        assert_eq!(
            titlebar.menus,
            vec!["文件", "编辑", "视图", "帮助"],
            "菜单标签与自绘标题栏一致"
        );
        assert!(model.right_sidebar.is_some(), "右侧栏仍应画在菜单栏下方");
    }

    // 工具栏弹层同理：精度 / 洋葱皮范围已由 egui 接管，不再让位。
    let mut snap = Root::new(&lumino_core::storage::config::UiConfig::default());
    snap.toolbar.snap_picker_open = true;
    assert!(
        snap.egui_chrome_model().toolbar.is_some(),
        "egui 精度下拉展开时仍应画工具栏 band"
    );

    let mut onion = Root::new(&lumino_core::storage::config::UiConfig::default());
    onion.toolbar.open_popover(Popover::OnionSkinMenu);
    assert!(onion.egui_chrome_model().toolbar.is_some());

    // 溢出菜单已由 egui 接管：不再让位（内容按真实宽度重算）。
    let mut overflow = Root::new(&lumino_core::storage::config::UiConfig::default());
    overflow.toolbar.open_popover(Popover::OverflowMenu);
    let model = overflow.egui_chrome_model_for(900.0);
    assert!(
        model.toolbar.is_some(),
        "egui 溢出菜单展开时仍应画工具栏 band"
    );
    assert!(
        model
            .toolbar
            .expect("工具栏")
            .overflow_menu
            .is_some_and(|menu| !menu.items.is_empty()),
        "900px 视口应产生溢出项"
    );

    // 悬浮工具条及其锚定面板已由 egui 接管：打开时不再让位，模型带胶囊。
    let mut toolbar = Root::new(&lumino_core::storage::config::UiConfig::default());
    toolbar.toolbar.tool_panel_open = true;
    let model = toolbar.egui_chrome_model();
    assert!(model.sidebar.is_some(), "悬浮条打开时仍应画外壳");
    let panel = model.tool_panel.expect("悬浮条应交给 egui");
    assert_eq!(panel.items.len(), 7, "七个绘制工具常显（含三点圆弧）");
    assert!(panel.panel.is_none(), "默认没有打开的面板");
}

mod about;
mod audio_export;
mod batch_edit;
mod brush_settings;
mod cloud_browser;
mod cloud_connect;
mod collaboration;
mod custom_precision;
mod dialog_draw045;
mod draw_settings;
mod export_progress;
mod help;
mod history;
mod key_colors;
mod load_confirm;
mod memory_monitor;
mod menu;
mod mixer;
mod part2;
mod project_settings;
mod recover_track;
mod right_sidebar;
mod save_confirm;
mod settings_audio;
mod settings_cloud;
mod settings_page;
mod shape_canvas;
mod shape_drawer;
mod sidebar_rename_blur;
mod speed_change;
mod tool_panel;
mod video_export;
mod video_export_overlay;
