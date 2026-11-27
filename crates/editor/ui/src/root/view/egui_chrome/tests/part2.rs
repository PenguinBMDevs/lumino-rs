//! 由 `tests.rs` 拆出的后半段测试，按原顺序保留。

use super::*;

#[test]
fn titlebar_menu_model_resolves_every_action() {
    let root = Root::new(&lumino_core::storage::config::UiConfig::default());
    let model = root.egui_chrome_model();
    let titlebar = model.titlebar.expect("自绘标题栏应有模型");
    let bar = titlebar.menu.expect("应带可交互菜单树");
    assert_eq!(bar.menus.len(), 4, "文件/编辑/视图/帮助");
    assert_eq!(
        bar.menus[0].label,
        lumino_core::i18n::main_translations(Language::ZhCn).menu_file
    );

    // 整棵树的每个动作序号都必须能解析回生产 `Message`。
    let mut actions = 0;
    for menu in &bar.menus {
        actions += check_entries(&root, &menu.entries);
    }
    assert_eq!(
        actions, 26,
        "15 文件（含最近打开空态占位）+ 7 编辑 + 3 视图 + 1 帮助"
    );
    assert!(root.menu_action_message(usize::MAX).is_none());

    // 空工程没有框选：导出为素材置灰。
    let translations = lumino_core::i18n::main_translations(Language::ZhCn);
    let export = bar.menus[0].entries.iter().find_map(|entry| match entry {
        MenuEntry::Action { label, enabled, .. } if *label == translations.file_export_material => {
            Some(*enabled)
        }
        _ => None,
    });
    assert_eq!(export, Some(false), "无框选时导出为素材应禁用");
}

fn check_entries(root: &Root, entries: &[MenuEntry]) -> usize {
    let mut count = 0;
    for entry in entries {
        match entry {
            MenuEntry::Action { action, .. } => {
                assert!(
                    root.menu_action_message(*action).is_some(),
                    "动作序号 {action} 必须能解析"
                );
                count += 1;
            }
            MenuEntry::Submenu { entries, .. } => count += check_entries(root, entries),
            MenuEntry::Separator => {}
        }
    }
    count
}

/// 有历史时文件菜单出现「最近打开」子菜单：条目按顺序、可解析回 `OpenRecent`，
/// 末尾是「清除最近记录」；没有历史时仍保留入口，展示一个禁用的空态占位项。
#[test]
fn file_menu_recent_submenu_tracks_recent_files() {
    // 路径用平台无关的相对形式：`Path::file_name` 只在 Windows 把 `\` 当分隔符，
    // 写死 Windows 路径会让 Linux/macOS 把整串路径当成文件名。
    let music = std::path::Path::new("Music");
    let first = music.join("a.mid");
    let second = music.join("b.lmpj");
    let config = lumino_core::storage::config::UiConfig {
        recent_files: vec![
            lumino_core::storage::config::RecentFile {
                path: first.to_string_lossy().into_owned(),
                opened_at: 2,
            },
            lumino_core::storage::config::RecentFile {
                path: second.to_string_lossy().into_owned(),
                opened_at: 1,
            },
        ],
        ..Default::default()
    };
    let root = Root::new(&config);
    let model = root.egui_chrome_model();
    let bar = model
        .titlebar
        .expect("自绘标题栏应有模型")
        .menu
        .expect("应带可交互菜单树");
    let translations = lumino_core::i18n::main_translations(Language::ZhCn);

    let recent = bar.menus[0]
        .entries
        .iter()
        .find_map(|entry| match entry {
            MenuEntry::Submenu { label, entries } if *label == translations.file_recent => {
                Some(entries)
            }
            _ => None,
        })
        .expect("有历史时应出现「最近打开」子菜单");
    assert_eq!(recent.len(), 4, "2 个历史条目 + 分隔线 + 1 个清除项");
    assert!(
        matches!(recent[2], MenuEntry::Separator),
        "历史条目与清除项之间应有分隔线"
    );

    let MenuEntry::Action { label, action, .. } = &recent[0] else {
        panic!("最近文件条目应为可点击动作");
    };
    assert_eq!(*label, "a.mid", "菜单只显示文件名");
    match root.menu_action_message(*action) {
        Some(Message::Core(crate::event::Event::Menu(crate::event::menu::Event::File(
            crate::event::menu::file::Event::OpenRecent(path),
        )))) => assert_eq!(path, first),
        _ => panic!("最近文件动作必须解析为 OpenRecent"),
    }

    let MenuEntry::Action { label, action, .. } = &recent[3] else {
        panic!("清除项应为可点击动作");
    };
    assert_eq!(*label, translations.file_recent_clear);
    assert!(
        matches!(
            root.menu_action_message(*action),
            Some(Message::Core(crate::event::Event::Menu(
                crate::event::menu::Event::File(crate::event::menu::file::Event::ClearRecent)
            )))
        ),
        "清除项必须解析为 ClearRecent"
    );

    // 空历史：入口仍可见，子菜单里只有一个禁用的占位项。
    let empty = Root::new(&lumino_core::storage::config::UiConfig::default());
    let empty_model = empty.egui_chrome_model();
    let empty_bar = empty_model
        .titlebar
        .expect("自绘标题栏应有模型")
        .menu
        .expect("应带可交互菜单树");
    let empty_recent = empty_bar.menus[0]
        .entries
        .iter()
        .find_map(|entry| match entry {
            MenuEntry::Submenu { label, entries } if *label == translations.file_recent => {
                Some(entries)
            }
            _ => None,
        })
        .expect("无历史时也应保留「最近打开」入口");
    assert_eq!(empty_recent.len(), 1, "空态只有占位项");
    let MenuEntry::Action {
        label,
        enabled,
        action,
    } = &empty_recent[0]
    else {
        panic!("空态占位项应为禁用动作");
    };
    assert_eq!(*label, translations.file_recent_empty);
    assert!(!*enabled, "空态占位项必须禁用");
    assert!(
        matches!(
            empty.menu_action_message(*action),
            Some(Message::Core(crate::event::Event::Menu(
                crate::event::menu::Event::File(crate::event::menu::file::Event::NoRecentFiles)
            )))
        ),
        "空态占位项必须解析为 NoRecentFiles"
    );
    let empty_actions: usize = empty_bar
        .menus
        .iter()
        .map(|menu| check_entries(&empty, &menu.entries))
        .sum();
    assert_eq!(
        empty_actions, 26,
        "空态占位不改变「每个动作都可解析」的契约"
    );
}

#[test]
fn toolbar_popup_model_and_actions_follow_production_state() {
    let mut root = Root::new(&lumino_core::storage::config::UiConfig::default());
    assert!(
        root.egui_chrome_model()
            .toolbar
            .expect("默认视图应渲染工具栏")
            .snap_menu
            .is_none(),
        "默认精度下拉关闭"
    );

    root.toolbar.snap_picker_open = true;
    let model = root.egui_chrome_model();
    let snap = model
        .toolbar
        .expect("默认视图应渲染工具栏")
        .snap_menu
        .expect("精度下拉应打开");
    assert_eq!(snap.items.len(), SnapSpec::presets().len(), "预设数量一致");
    assert_eq!(
        snap.items.iter().filter(|item| item.selected).count(),
        1,
        "恰有一个当前规格"
    );
    // 自定义项文案跟随运行时语言（中文「自定义…」/ 英文「Custom…」）；
    // 只锁两种合法取值，避免并行测试切换全局语言时偶发失败。
    assert!(
        matches!(snap.custom_label.as_str(), "自定义…" | "Custom…"),
        "自定义项应为本地化文案：{:?}",
        snap.custom_label
    );

    root.toolbar.snap_picker_open = false;
    root.toolbar.onion_skin_menu_open = true;
    let model = root.egui_chrome_model();
    let onion = model
        .toolbar
        .expect("默认视图应渲染工具栏")
        .onion_menu
        .expect("洋葱皮范围菜单应打开");
    assert_eq!(onion.items.len(), 4, "全部/上一个/下一个/指定");
    assert_eq!(onion.items[0].label, "显示全部音轨");
    assert!(onion.items[0].selected, "默认选中「显示全部音轨」");
    assert!(!onion.items[1].selected);

    // 动作 → 生产 Message（与 上一代 按钮同一批构造器）。
    assert!(matches!(
        root.toolbar_action_message(&ToolbarAction::ToggleSnapPicker),
        Some(Message::Toolbar(ToolbarEvent::ToggleSnapPicker))
    ));
    let triple = SnapSpec::Note {
        notes: 8,
        dots: 0,
        div: 3,
    };
    assert!(matches!(
        root.toolbar_action_message(&ToolbarAction::SnapSpec(triple)),
        Some(Message::Toolbar(ToolbarEvent::SnapSpecChanged(spec))) if spec == triple
    ));
    assert!(matches!(
        root.toolbar_action_message(&ToolbarAction::SnapCustom),
        Some(Message::Batch(_))
    ));
    assert!(matches!(
        root.toolbar_action_message(&ToolbarAction::ToggleOnionSkin),
        Some(Message::Toolbar(ToolbarEvent::ToggleOnionSkin))
    ));
    assert!(matches!(
        root.toolbar_action_message(&ToolbarAction::OnionMode(1)),
        Some(Message::Toolbar(ToolbarEvent::OnionSkinModeSelected(
            OnionSkinMode::PreviousTrack
        )))
    ));
    assert!(
        root.toolbar_action_message(&ToolbarAction::OnionMode(4))
            .is_none(),
        "越界下标不得产生消息"
    );
}

#[test]
fn toolbar_overflow_follows_width_and_resolves_items() {
    let mut root = Root::new(&lumino_core::storage::config::UiConfig::default());

    // 宽窗口：不折叠、无「更多」、协作按钮可见。
    let wide = root.egui_chrome_model_for(2400.0).toolbar.expect("工具栏");
    assert!(!wide.more_visible, "宽窗口不应出现更多按钮");
    assert!(wide.collaboration_visible, "无溢出时协作按钮常显");
    assert_eq!(wide.visible_tools, 14, "工具区全部可见");
    assert!(wide.groups.dashboard && wide.groups.tools);
    assert!(wide.overflow_menu.is_none());

    // 窄窗口：折叠 + 「更多」+ 协作被收进菜单；菜单打开后有项。
    root.toolbar.overflow_menu_open = true;
    let narrow = root.egui_chrome_model_for(900.0).toolbar.expect("工具栏");
    assert!(narrow.more_visible, "窄窗口应出现更多按钮");
    assert!(!narrow.collaboration_visible, "溢出时协作最先折叠");
    assert!(narrow.visible_tools < 14, "窄窗口应逐项折叠工具");
    let menu = narrow.overflow_menu.expect("溢出菜单应打开");
    assert!(!menu.items.is_empty());

    // 每个可用项都能解析回生产 Message；禁用项不产生消息。
    let mut enabled = 0;
    let mut disabled = 0;
    for (index, item) in menu.items.iter().enumerate() {
        let message = root.overflow_action_message(index, 900.0);
        if item.enabled {
            assert!(message.is_some(), "第 {index} 项应可解析");
            enabled += 1;
        } else {
            assert!(message.is_none(), "禁用项不得产生消息");
            disabled += 1;
        }
    }
    assert!(enabled > 0);
    assert!(disabled > 0, "无选中时翻转/移调应置灰并出现在菜单里");
    assert!(
        root.overflow_action_message(menu.items.len(), 900.0)
            .is_none(),
        "越界下标不得产生消息"
    );
}

#[test]
fn egui_palette_follows_the_window_theme() {
    // 默认配置就是 Light：外壳必须派生浅色，不能固定为深色。
    let light = Root::new(&lumino_core::storage::config::UiConfig::default());
    let palette = light.egui_palette();
    assert!(!palette.is_dark, "默认配置（Light）必须派生浅色外壳");
    assert_eq!(
        palette.titlebar_bg,
        egui::Color32::from_rgb(0xD6, 0xD6, 0xD6),
        "标题栏应取 Light 的 background.neutral"
    );
    assert_ne!(palette, Palette::TOKYO_NIGHT_STORM);

    // 历史基线主题必须逐色还原（场景像素基线依赖它）。
    let storm = Root::new(&lumino_core::storage::config::UiConfig {
        theme: lumino_ui_core::Theme::TokyoNightStorm.to_string(),
        ..Default::default()
    });
    assert_eq!(
        storm.egui_palette(),
        Palette::TOKYO_NIGHT_STORM,
        "Tokyo Night Storm 必须精确还原历史色板"
    );

    // 高对比度主题走自定义色板：黑底 + 黄强调。
    let hc = Root::new(&lumino_core::storage::config::UiConfig {
        theme: lumino_ui_core::theme::HIGH_CONTRAST_DISPLAY.to_owned(),
        ..Default::default()
    });
    let palette = hc.egui_palette();
    assert!(palette.is_dark, "高对比度是深色主题");
    assert_eq!(palette.base_bg, egui::Color32::BLACK, "高对比度底为纯黑");
}
