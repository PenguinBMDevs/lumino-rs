//! 右侧栏面板：模型镜像与动作翻译。

use super::*;
use lumino_message::RightSidebarAction as SidebarAction;
use lumino_ui_egui::chrome::right_panel::RightSidebarAction as PanelAction;
use lumino_ui_egui::chrome::{
    I2mControlModel, I2mField, I2mSelectField, MaterialContextMenuItem, MaterialMenuKind,
    MaterialsPanelModel, RightSidebarPanelKind, RightSidebarPanelModel,
};

use crate::Message;
use crate::right_sidebar::material::{MaterialEntry, MaterialSource};
use crate::right_sidebar::{RightSidebarPanel, palette_algorithms};

fn root() -> Root {
    Root::new(&lumino_core::storage::config::UiConfig::default())
}

fn i2m_panel(root: &mut Root) -> lumino_ui_egui::chrome::I2mPanelModel {
    let model = root.egui_chrome_model();
    let right = model.right_sidebar.expect("默认视图应渲染右侧栏");
    assert_eq!(right.active, Some(RightSidebarPanelKind::ImageToMidi));
    let Some(RightSidebarPanelModel::ImageToMidi(panel)) = right.content else {
        panic!("图片转 MIDI 面板应由 egui 接管");
    };
    panel
}

#[test]
fn i2m_model_mirrors_config_and_selection() {
    let mut root = root();
    root.right_sidebar.panel_visible = true;
    root.right_sidebar.active_panel = RightSidebarPanel::ImageToMidi;
    root.right_sidebar.config.target_height_text = "42".to_owned();
    root.right_sidebar.config.palette_index = 3;

    let panel = i2m_panel(&mut root);
    assert_eq!(panel.title, "图片转 MIDI");
    assert_eq!(panel.select_image, "选择图片文件");
    assert!(panel.selected_hint.is_none());
    assert_eq!(
        panel.rows.len(),
        17,
        "5 数字 + 最长音符 + 2 小节 + 6 算法下拉"
    );
    assert!(matches!(
        &panel.rows[0].control,
        I2mControlModel::KeyRange {
            start_field: I2mField::StartKey,
            start_text,
            end_field: I2mField::EndKey,
            end_text,
        } if start_text == "21" && end_text == "108"
    ));
    assert!(matches!(
        &panel.rows[1].control,
        I2mControlModel::Input {
            field: I2mField::TargetHeight,
            text,
        } if text == "42"
    ));
    assert!(matches!(
        &panel.rows[4].control,
        I2mControlModel::Input {
            field: I2mField::MaxNoteLength,
            ..
        }
    ));
    assert!(matches!(
        &panel.rows[5].control,
        I2mControlModel::Section { .. }
    ));
    assert!(matches!(
        &panel.rows[11].control,
        I2mControlModel::Select {
            field: I2mSelectField::Palette,
            options: list,
            selected,
        } if list.len() == palette_algorithms().len() && *selected == 3
    ));
    assert!(panel.convert.is_none(), "未选择文件不应有转换按钮");

    // 选择文件后出现转换按钮；转换中禁用且改文案。
    root.right_sidebar.selected_image_path = Some(std::path::PathBuf::from("demo.png"));
    let panel = i2m_panel(&mut root);
    let convert = panel.convert.expect("应出现转换按钮");
    assert!(convert.enabled);
    assert!(panel.selected_hint.is_some());
    let idle_label = convert.label;

    root.right_sidebar.converting = true;
    let panel = i2m_panel(&mut root);
    let convert = panel.convert.expect("转换中按钮仍在");
    assert!(!convert.enabled);
    assert_ne!(convert.label, idle_label, "转换中应改文案");

    // 素材库与钢琴瀑布流也已接管；瀑布流内容是外部离屏纹理。
    root.right_sidebar.active_panel = RightSidebarPanel::Materials;
    let model = root.egui_chrome_model();
    let right = model.right_sidebar.expect("右侧栏");
    assert!(
        matches!(right.content, Some(RightSidebarPanelModel::Materials(_))),
        "素材库内容应由 egui 接管"
    );
    root.right_sidebar.active_panel = RightSidebarPanel::PianoWaterfall;
    let model = root.egui_chrome_model();
    let right = model.right_sidebar.expect("右侧栏");
    assert!(
        matches!(
            right.content,
            Some(RightSidebarPanelModel::PianoWaterfall(_))
        ),
        "瀑布流面板骨架/内容也应由 egui 接管"
    );
}

#[test]
fn panel_actions_map_to_right_sidebar_messages() {
    let root = root();

    assert!(matches!(
        root.right_sidebar_action_message(&PanelAction::SelectImageFile),
        Some(Message::RightSidebar(SidebarAction::SelectImageFile))
    ));
    assert!(matches!(
        root.right_sidebar_action_message(&PanelAction::I2mConfigTextChanged {
            field: I2mField::EndKey,
            text: "120".to_owned(),
        }),
        Some(Message::RightSidebar(SidebarAction::I2mConfigTextChanged {
            field: lumino_message::I2mConfigField::EndKey,
            text,
        })) if text == "120"
    ));
    assert!(matches!(
        root.right_sidebar_action_message(&PanelAction::I2mSelectionChanged {
            field: I2mSelectField::Palette,
            index: 7,
        }),
        Some(Message::RightSidebar(SidebarAction::I2mSelectionChanged {
            field: lumino_message::I2mSelectField::Palette,
            index: 7,
        }))
    ));
    assert!(matches!(
        root.right_sidebar_action_message(&PanelAction::ConvertClicked),
        Some(Message::RightSidebar(SidebarAction::ConvertClicked))
    ));
}

fn materials_panel(root: &mut Root) -> MaterialsPanelModel {
    let model = root.egui_chrome_model();
    let right = model.right_sidebar.expect("默认视图应渲染右侧栏");
    assert_eq!(right.active, Some(RightSidebarPanelKind::Materials));
    let Some(RightSidebarPanelModel::Materials(panel)) = right.content else {
        panic!("素材库面板应由 egui 接管");
    };
    panel
}

fn push_entry(root: &mut Root, name: &str, source: MaterialSource, valid: bool) {
    root.right_sidebar.materials.entries.push(MaterialEntry {
        name: name.to_owned(),
        author: "作者".to_owned(),
        source,
        path: Some(std::path::PathBuf::from("C:/materials/a.lmmaterial")),
        data: None,
        multi_track: false,
        track_count: 2,
        valid,
        preview: None,
    });
}

#[test]
fn materials_model_mirrors_library_state() {
    let mut root = root();
    root.right_sidebar.panel_visible = true;
    root.right_sidebar.active_panel = RightSidebarPanel::Materials;
    push_entry(&mut root, "内置 A", MaterialSource::BuiltIn, true);
    push_entry(&mut root, "损坏素材", MaterialSource::BuiltIn, false);
    push_entry(&mut root, "我的素材", MaterialSource::User, true);
    root.right_sidebar.materials.add_menu_open = true;
    root.right_sidebar.materials.renaming_material = Some((2, "新名称".to_owned()));
    root.right_sidebar.materials.context_menu_target = Some(2);
    root.right_sidebar.materials.context_menu_pos = Some((30.0, 40.0));

    let panel = materials_panel(&mut root);
    assert_eq!(panel.title, "素材库");
    assert!(panel.add_label.starts_with("+ "));
    assert_eq!(panel.rename_placeholder, "素材名称");
    assert_eq!(panel.sections.len(), 2, "内置 + 用户两个分区");
    assert_eq!(panel.sections[0].entries.len(), 2);
    assert!(!panel.sections[0].entries[1].valid);
    assert_eq!(
        panel.sections[1].entries[0].renaming.as_deref(),
        Some("新名称")
    );
    assert!(
        panel.sections[0].entries[0]
            .tooltip
            .iter()
            .any(|line| line.contains("内置 A")),
        "悬停提示应含名称"
    );
    assert!(
        !panel.sections[0].entries[1].tooltip.is_empty(),
        "无效素材也有提示"
    );
    let menu = panel.add_menu.expect("添加菜单应展开");
    assert_eq!(menu.len(), 2);
    assert_eq!(menu[0].kind, MaterialMenuKind::DownloadFromWeb);
    let ctx = panel.context_menu.expect("右键菜单应展开");
    assert_eq!(ctx.index, 2);
    assert!(ctx.can_edit, "用户素材可编辑");
    assert_eq!((ctx.x, ctx.y), (30.0, 40.0));

    // 内置素材的右键菜单置灰。
    root.right_sidebar.materials.context_menu_target = Some(0);
    let panel = materials_panel(&mut root);
    assert!(panel.context_menu.is_some_and(|ctx| !ctx.can_edit));

    // 空列表：未初始化显示「素材无效」，已初始化显示分区占位。
    root.right_sidebar.materials.entries.clear();
    root.right_sidebar.materials.scanning = false;
    let panel = materials_panel(&mut root);
    assert!(panel.placeholder.is_some());
    root.right_sidebar.materials.scanning = true;
    let panel = materials_panel(&mut root);
    assert!(panel.placeholder.is_some());
}

#[test]
fn materials_actions_map_to_messages() {
    let root = root();

    assert!(matches!(
        root.right_sidebar_action_message(&PanelAction::MaterialAdd),
        Some(Message::RightSidebar(SidebarAction::MaterialAddClicked))
    ));
    assert!(matches!(
        root.right_sidebar_action_message(&PanelAction::MaterialMenu(
            MaterialMenuKind::DownloadFromWeb
        )),
        Some(Message::RightSidebar(
            SidebarAction::MaterialDownloadFromWeb
        ))
    ));
    assert!(matches!(
        root.right_sidebar_action_message(&PanelAction::MaterialMenu(
            MaterialMenuKind::ImportFromLocal
        )),
        Some(Message::RightSidebar(
            SidebarAction::MaterialImportFromLocal
        ))
    ));
    assert!(matches!(
        root.right_sidebar_action_message(&PanelAction::MaterialDragStarted(3)),
        Some(Message::RightSidebar(SidebarAction::MaterialDragStarted(3)))
    ));
    assert!(matches!(
        root.right_sidebar_action_message(&PanelAction::MaterialContextMenuClosed),
        Some(Message::RightSidebar(
            SidebarAction::MaterialContextMenuClosed
        ))
    ));
    assert!(matches!(
        root.right_sidebar_action_message(&PanelAction::MaterialRenameInput(
            "新名称".to_owned()
        )),
        Some(Message::RightSidebar(
            SidebarAction::MaterialRenameInputChanged(value)
        )) if value == "新名称"
    ));
    assert!(matches!(
        root.right_sidebar_action_message(&PanelAction::MaterialRenameConfirmed),
        Some(Message::RightSidebar(
            SidebarAction::MaterialRenameConfirmed
        ))
    ));

    // 右键菜单项逐项映射。
    for (item, expected) in [
        (
            MaterialContextMenuItem::Rename,
            lumino_message::MaterialContextMenuItem::Rename,
        ),
        (
            MaterialContextMenuItem::Delete,
            lumino_message::MaterialContextMenuItem::Delete,
        ),
        (
            MaterialContextMenuItem::UploadToCloud,
            lumino_message::MaterialContextMenuItem::UploadToCloud,
        ),
    ] {
        assert!(matches!(
            root.right_sidebar_action_message(&PanelAction::MaterialContextMenuItemClicked {
                index: 4,
                item,
            }),
            Some(Message::RightSidebar(
                SidebarAction::MaterialContextMenuItemClicked(4, got)
            )) if got == expected
        ));
    }

    // 右键打开：先同步局部光标，再打开菜单（顺序不可交换）。
    let Some(Message::Batch(items)) =
        root.right_sidebar_action_message(&PanelAction::MaterialContextMenuOpened {
            index: 2,
            x: 12.5,
            y: 34.5,
        })
    else {
        panic!("右键打开应产生 Batch");
    };
    assert_eq!(items.len(), 2);
    assert!(matches!(
        &items[0],
        Message::RightSidebar(SidebarAction::MaterialCursorMoved(x, y))
            if (*x - 12.5).abs() < f32::EPSILON && (*y - 34.5).abs() < f32::EPSILON
    ));
    assert!(matches!(
        &items[1],
        Message::RightSidebar(SidebarAction::MaterialContextMenuOpened(2))
    ));
}

#[test]
fn rail_items_are_filled_in_legacy_order() {
    let root = root();
    let model = root.egui_chrome_model();
    let right = model.right_sidebar.expect("右侧栏应可见");
    let kinds: Vec<RightSidebarPanelKind> = right.rail.iter().map(|item| item.kind).collect();
    assert_eq!(
        kinds,
        vec![
            RightSidebarPanelKind::ImageToMidi,
            RightSidebarPanelKind::Materials,
            RightSidebarPanelKind::PianoWaterfall,
        ],
        "图标列顺序必须与 旧版 `view.rs` 一致（图片转 MIDI / 素材库 / 瀑布流）"
    );
    assert!(
        right.rail.iter().all(|item| !item.label.is_empty()),
        "每个图标都要有悬停提示文案"
    );
}

#[test]
fn rail_clicks_map_to_panel_toggles() {
    let root = root();
    assert!(matches!(
        root.right_sidebar_action_message(&PanelAction::RailClicked(
            RightSidebarPanelKind::ImageToMidi
        )),
        Some(Message::RightSidebar(SidebarAction::ImageToMidiClicked))
    ));
    assert!(matches!(
        root.right_sidebar_action_message(&PanelAction::RailClicked(
            RightSidebarPanelKind::Materials
        )),
        Some(Message::RightSidebar(SidebarAction::MaterialLibraryClicked))
    ));
    assert!(matches!(
        root.right_sidebar_action_message(&PanelAction::RailClicked(
            RightSidebarPanelKind::PianoWaterfall
        )),
        Some(Message::RightSidebar(SidebarAction::PianoWaterfallClicked))
    ));
}

#[test]
fn resize_handle_actions_map_to_drag_events() {
    let root = root();
    assert!(matches!(
        root.right_sidebar_action_message(&PanelAction::ResizeStarted),
        Some(Message::RightSidebar(SidebarAction::ResizeDragStarted))
    ));
    assert!(matches!(
        root.right_sidebar_action_message(&PanelAction::ResizeEnded),
        Some(Message::RightSidebar(SidebarAction::ResizeDragEnded))
    ));
}

#[test]
fn waterfall_panel_model_carries_title_and_texture_state() {
    let mut root = root();
    root.right_sidebar.panel_visible = true;
    root.right_sidebar.active_panel = RightSidebarPanel::PianoWaterfall;

    let model = root.egui_chrome_model();
    let right = model.right_sidebar.expect("右侧栏应可见");
    assert_eq!(
        right.active,
        Some(RightSidebarPanelKind::PianoWaterfall),
        "激活图标应指向钢琴瀑布流"
    );
    let Some(RightSidebarPanelModel::PianoWaterfall(panel)) = right.content else {
        panic!("钢琴瀑布流面板应由 egui 接管（内容是外部离屏纹理）");
    };
    assert_eq!(panel.title, "钢琴瀑布流预览");
    assert!(!panel.empty_hint.is_empty(), "纹理未就绪时要有占位文案");
    assert!(
        panel.texture.is_none(),
        "无头测试没有 gfx 纹理，模型不得凭空携带纹理 ID"
    );
}
