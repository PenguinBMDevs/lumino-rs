//! 形状抽屉左侧面板的无头测试：库选择 / 文件与编辑动作 / 面积 / 插入与缩放。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lumino_ui_egui::chrome::{
    ShapeActionButton, ShapeDrawerModel, ShapePanelAction, ShapeRowModel, ShapeSwatchModel,
};
use lumino_ui_egui::shape_drawer::{self, ShapeDrawerAction};

fn model() -> ShapeDrawerModel {
    ShapeDrawerModel {
        titlebar: false,
        rect: egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(250.0, 700.0)),
        title: "Shape Library".to_owned(),
        rows: vec![
            ShapeRowModel {
                id: "builtin/star".to_owned(),
                label: "● Star".to_owned(),
                selected: true,
            },
            ShapeRowModel {
                id: "builtin/heart".to_owned(),
                label: "Heart".to_owned(),
                selected: false,
            },
        ],
        name_input: "New Shape".to_owned(),
        name_hint: "Shape name".to_owned(),
        file_actions: vec![
            ShapeActionButton {
                label: "New".to_owned(),
                action: ShapePanelAction::New,
            },
            ShapeActionButton {
                label: "Save".to_owned(),
                action: ShapePanelAction::Save,
            },
            ShapeActionButton {
                label: "Rename".to_owned(),
                action: ShapePanelAction::Rename,
            },
            ShapeActionButton {
                label: "Del".to_owned(),
                action: ShapePanelAction::Delete,
            },
        ],
        edit_actions: vec![
            ShapeActionButton {
                label: "Copy".to_owned(),
                action: ShapePanelAction::Copy,
            },
            ShapeActionButton {
                label: "Paste".to_owned(),
                action: ShapePanelAction::Paste,
            },
            ShapeActionButton {
                label: "FlipH".to_owned(),
                action: ShapePanelAction::FlipHorizontal,
            },
            ShapeActionButton {
                label: "FlipV".to_owned(),
                action: ShapePanelAction::FlipVertical,
            },
            ShapeActionButton {
                label: "Rot90".to_owned(),
                action: ShapePanelAction::Rotate90,
            },
        ],
        area_toggle_label: "Area".to_owned(),
        area_mode: false,
        area_clear: "Clear".to_owned(),
        area_hint: "single closed stroke only".to_owned(),
        swatches: (0..=15u8)
            .map(|colour| ShapeSwatchModel {
                colour,
                label: format!("{colour}"),
                selected: false,
            })
            .collect(),
        status: None,
        warning: None,
        size_label: "Size".to_owned(),
        insert_beats: "4".to_owned(),
        insert_beats_hint: "beats".to_owned(),
        times_label: "x".to_owned(),
        insert_keys: "12".to_owned(),
        insert_keys_hint: "keys".to_owned(),
        fit_label: "Stretch".to_owned(),
        fit_contain: false,
        scale_label: "Scale".to_owned(),
        scale_percent: "100".to_owned(),
        scale_hint: "%".to_owned(),
        scale_apply: "Apply".to_owned(),
        bend: None,
        bend_corner: false,
        insert: "Insert".to_owned(),
    }
}

fn make_harness(
    model: ShapeDrawerModel,
) -> (Harness<'static>, Rc<RefCell<Vec<ShapeDrawerAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(250.0, 700.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            shape_drawer::show(ui, &model.clone());
            if let Some(action) = shape_drawer::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    (harness, actions)
}

#[test]
fn panel_shows_library_and_actions() {
    let (harness, _) = make_harness(model());
    for label in [
        "Shape Library",
        "New",
        "Save",
        "Rename",
        "Del",
        "Copy",
        "Paste",
        "FlipH",
        "FlipV",
        "Rot90",
        "Area",
        "Clear",
        "Size",
        "Scale",
        "Apply",
        "Insert",
    ] {
        assert!(
            harness.query_by_label(label).is_some(),
            "缺少可访问标签：{label}"
        );
    }
    assert!(harness.query_all_by_label("● Star").count() >= 1);
    assert!(harness.query_all_by_label("Heart").count() >= 1);
}

#[test]
fn library_click_dispatches_select() {
    let (mut harness, actions) = make_harness(model());
    harness
        .get_all_by_label("Heart")
        .next()
        .expect("库行应可定位")
        .click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&ShapeDrawerAction::Panel(ShapePanelAction::Select(
            "builtin/heart".to_owned()
        )))
    );
}

#[test]
fn file_and_edit_buttons_dispatch() {
    let (mut harness, actions) = make_harness(model());
    harness.get_by_label("Save").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&ShapeDrawerAction::Panel(ShapePanelAction::Save))
    );

    let (mut harness, actions) = make_harness(model());
    harness.get_by_label("Rot90").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&ShapeDrawerAction::Panel(ShapePanelAction::Rotate90))
    );
}

#[test]
fn area_toggle_and_swatch_dispatch() {
    let (mut harness, actions) = make_harness(model());
    harness.get_by_label("Area").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&ShapeDrawerAction::Panel(ShapePanelAction::ToggleAreaMode))
    );

    let (mut harness, actions) = make_harness(model());
    harness.get_by_label("7").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&ShapeDrawerAction::Panel(ShapePanelAction::SetAreaColour(
            7
        )))
    );
}

#[test]
fn insert_and_scale_actions_dispatch() {
    let (mut harness, actions) = make_harness(model());
    harness.get_by_label("Insert").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&ShapeDrawerAction::Panel(ShapePanelAction::Insert))
    );

    let (mut harness, actions) = make_harness(model());
    harness.get_by_label("Apply").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&ShapeDrawerAction::Panel(ShapePanelAction::ApplyScale))
    );
}

#[test]
fn name_input_dispatches_text() {
    let (mut harness, actions) = make_harness(model());
    let input = harness.get_by(|node| {
        node.role() == egui::Role::TextInput && node.value().as_deref() == Some("New Shape")
    });
    input.focus();
    input.type_text("X");
    harness.run();
    assert!(
        actions.borrow().iter().any(|action| matches!(
            action,
            ShapeDrawerAction::Panel(ShapePanelAction::SetName(text)) if text.contains('X')
        )),
        "名称输入应回传 SetName，实际 {:?}",
        actions.borrow()
    );
}

#[test]
fn bend_button_only_when_curve_anchor() {
    let (harness, _) = make_harness(model());
    assert!(harness.query_by_label("Bend: Corner").is_none());

    let mut model = model();
    model.bend = Some("Bend: Corner".to_owned());
    model.bend_corner = true;
    let (mut harness, actions) = make_harness(model);
    harness.get_by_label("Bend: Corner").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&ShapeDrawerAction::Panel(
            ShapePanelAction::ToggleAnchorBend
        ))
    );
}
