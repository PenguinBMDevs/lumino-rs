//! 右侧栏面板：模型镜像与动作翻译。

use super::*;
use lumino_message::RightSidebarAction as SidebarAction;
use lumino_ui_egui::chrome::right_panel::RightSidebarAction as PanelAction;
use lumino_ui_egui::chrome::{
    I2mControlModel, I2mField, RightSidebarPanelKind, RightSidebarPanelModel,
};

use crate::Message;
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
        5,
        "Key 范围 / 高度 / tick / 颜色数 / 调色板"
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
    let options = palette_algorithms().len();
    assert!(matches!(
        &panel.rows[4].control,
        I2mControlModel::Palette { options: list, selected } if list.len() == options && *selected == 3
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

    // 只接管 I2M：素材库 / 瀑布流仍由 iced 绘制。
    root.right_sidebar.active_panel = RightSidebarPanel::Materials;
    let model = root.egui_chrome_model();
    let right = model.right_sidebar.expect("右侧栏");
    assert!(right.content.is_none(), "素材库内容尚未接管");
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
        root.right_sidebar_action_message(&PanelAction::I2mPaletteChanged(7)),
        Some(Message::RightSidebar(SidebarAction::I2mPaletteChanged(7)))
    ));
    assert!(matches!(
        root.right_sidebar_action_message(&PanelAction::ConvertClicked),
        Some(Message::RightSidebar(SidebarAction::ConvertClicked))
    ));
}
