//! 形状抽屉面板：模型镜像与动作映射。

use super::*;
use crate::Message;
use crate::message::ShapeDrawerAction as Prod;
use lumino_ui_egui::chrome::ShapePanelAction;
use lumino_ui_egui::shape_drawer::ShapeDrawerAction as EguiAction;

fn dialog_root() -> Root {
    Root::new_dialog(
        &lumino_ui_core::Theme::TokyoNightStorm.to_string(),
        crate::state::root_state::DialogType::ShapeDrawer,
    )
}

fn rect() -> egui::Rect {
    egui::Rect::from_min_size(egui::pos2(0.0, 30.0), egui::vec2(250.0, 700.0))
}

#[test]
fn panel_model_mirrors_state() {
    let mut root = dialog_root();
    root.state.shape_drawer.name_input = "Star".to_owned();
    root.state.shape_drawer.insert_beats_text = "8".to_owned();
    root.state.shape_drawer.insert_keys_text = "24".to_owned();
    root.state.shape_drawer.scale_percent_text = "150".to_owned();
    root.state.shape_drawer.status = Some("已保存".to_owned());
    // 用真实库 id，避免依赖内置图形的命名。
    let first_id = root
        .state
        .shape_drawer
        .library
        .all()
        .next()
        .map(|drawing| drawing.id.clone())
        .expect("内置库不应为空");
    root.state.shape_drawer.selected = Some(first_id);

    let model = root.shape_drawer_model(rect());
    assert_eq!(model.rect, rect());
    assert_eq!(model.title, "Shape Library");
    assert_eq!(model.name_input, "Star");
    assert_eq!(model.name_hint, "Shape name");
    assert_eq!(model.insert_beats, "8");
    assert_eq!(model.insert_keys, "24");
    assert_eq!(model.scale_percent, "150");
    assert_eq!(model.status.as_deref(), Some("已保存"));
    assert_eq!(model.area_toggle_label, "Area");
    assert!(!model.area_mode);
    assert_eq!(model.fit_label, "Stretch");
    assert_eq!(model.swatches.len(), 16);
    assert!(model.bend.is_none(), "未选中曲线锚点时不显示弯折按钮");

    // 库条目：选中项带「● 」前缀。
    let selected: Vec<&str> = model
        .rows
        .iter()
        .filter(|row| row.selected)
        .map(|row| row.label.as_str())
        .collect();
    assert_eq!(selected.len(), 1);
    assert!(selected[0].starts_with("● "));

    // 动作按钮：文件 4 个、编辑 5 个。
    assert_eq!(model.file_actions.len(), 4);
    assert_eq!(model.file_actions[0].label, "New");
    assert_eq!(model.file_actions[3].action, ShapePanelAction::Delete);
    assert_eq!(model.edit_actions.len(), 5);
    assert_eq!(model.edit_actions[4].label, "Rot90");
}

#[test]
fn area_mode_reflects_doc() {
    let root = dialog_root();
    {
        let doc = root.state.shape_drawer.doc.clone();
        let mut doc = doc.borrow_mut();
        doc.area_mode = true;
        doc.area_colour = 5;
    }
    let model = root.shape_drawer_model(rect());
    assert!(model.area_mode);
    assert_eq!(model.area_toggle_label, "Area: on");
    assert_eq!(model.swatches[5].colour, 5);
    assert!(model.swatches[5].selected, "当前面积色高亮");
    assert!(!model.swatches[0].selected);
    // 无闭合描边时提示为「single closed stroke only」，与生产一致。
    assert!(
        model.area_hint.contains("colour") || model.area_hint.contains("closed stroke"),
        "面积提示文案异常：{}",
        model.area_hint
    );
}

#[test]
fn actions_map_to_shape_drawer_messages() {
    let root = dialog_root();
    let cases = [
        (ShapePanelAction::Select("id".to_owned()), "Select"),
        (ShapePanelAction::New, "New"),
        (ShapePanelAction::Save, "Save"),
        (ShapePanelAction::Rename, "Rename"),
        (ShapePanelAction::Delete, "Delete"),
        (ShapePanelAction::Copy, "Copy"),
        (ShapePanelAction::Paste, "Paste"),
        (ShapePanelAction::FlipHorizontal, "FlipHorizontal"),
        (ShapePanelAction::FlipVertical, "FlipVertical"),
        (ShapePanelAction::Rotate90, "Rotate90"),
        (ShapePanelAction::ToggleAreaMode, "ToggleAreaMode"),
        (ShapePanelAction::ClearAreas, "ClearAreas"),
        (ShapePanelAction::SetAreaColour(3), "SetAreaColour"),
        (ShapePanelAction::SetName("n".to_owned()), "SetName"),
        (
            ShapePanelAction::SetInsertBeats("2".to_owned()),
            "SetInsertBeats",
        ),
        (
            ShapePanelAction::SetInsertKeys("5".to_owned()),
            "SetInsertKeys",
        ),
        (ShapePanelAction::ToggleInsertFit, "ToggleInsertFit"),
        (
            ShapePanelAction::SetScalePercent("80".to_owned()),
            "SetScalePercent",
        ),
        (ShapePanelAction::ApplyScale, "ApplyScale"),
        (ShapePanelAction::ToggleAnchorBend, "ToggleAnchorBend"),
        (ShapePanelAction::Insert, "Insert"),
    ];
    for (action, name) in cases {
        let message = root
            .shape_drawer_action_message(&EguiAction::Panel(action))
            .unwrap_or_else(|| panic!("{name} 应映射为消息"));
        let text = format!("{message:?}");
        assert!(text.contains(name), "{name} 映射错误：{text}");
    }

    // 具体载荷核对。
    assert!(matches!(
        root.shape_drawer_action_message(&EguiAction::Panel(ShapePanelAction::Select(
            "id".to_owned()
        ))),
        Some(Message::ShapeDrawer(Prod::Select(id))) if id == "id"
    ));
    assert!(matches!(
        root.shape_drawer_action_message(&EguiAction::Panel(ShapePanelAction::SetAreaColour(9))),
        Some(Message::ShapeDrawer(Prod::SetAreaColour(9)))
    ));
}
