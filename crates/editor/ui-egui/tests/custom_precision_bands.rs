//! 自定义精度对话框的无头交互测试：输入回传 + 附点切换 + 按钮。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lumino_core::DotType;
use lumino_ui_egui::chrome::{
    self, ChromeModel, CustomPrecisionModel, DialogModel, DotOptionModel,
};
use lumino_ui_egui::custom_precision::CustomPrecisionAction;

fn dots(selected: DotType) -> Vec<DotOptionModel> {
    [
        (DotType::None, "（无）"),
        (DotType::Tuplet, "连音"),
        (DotType::Single, "附点"),
        (DotType::Double, "双附点"),
    ]
    .into_iter()
    .map(|(dot, label)| DotOptionModel {
        dot,
        label: label.to_owned(),
        selected: dot == selected,
    })
    .collect()
}

fn model() -> CustomPrecisionModel {
    CustomPrecisionModel {
        titlebar: false,
        tuplet_count: "3".to_owned(),
        note_value: "4".to_owned(),
        note_label: "分音符".to_owned(),
        dot_label: "附点".to_owned(),
        dots: dots(DotType::None),
        divide_label: "除以".to_owned(),
        divisor: "1".to_owned(),
        error: None,
        ok: "确定".to_owned(),
        cancel: "取消".to_owned(),
    }
}

fn make_harness(
    model: CustomPrecisionModel,
) -> (Harness<'static>, Rc<RefCell<Vec<CustomPrecisionAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(500.0, 260.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            let model = ChromeModel {
                dialog: Some(DialogModel::CustomPrecision(Box::new(model.clone()))),
                ..ChromeModel::default()
            };
            chrome::show_bands(ui, &model, &mut icons);
            if let Some(action) = lumino_ui_egui::custom_precision::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    (harness, actions)
}

/// 确定 / 取消各自回传对应动作。
#[test]
fn buttons_dispatch_confirm_and_cancel() {
    let (mut harness, actions) = make_harness(model());
    harness.get_by_label("确定").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&CustomPrecisionAction::Confirm)
    );

    let (mut harness, actions) = make_harness(model());
    harness.get_by_label("取消").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&CustomPrecisionAction::Cancel)
    );
}

/// 三个输入框各自回传正确的字段（三连音数 / 分音符 / 除数）。
#[test]
fn inputs_dispatch_their_fields() {
    let (mut harness, actions) = make_harness(model());
    let input = harness.get_by(|node| {
        node.role() == egui::Role::TextInput && node.value().as_deref() == Some("4")
    });
    input.focus();
    input.type_text("6");
    harness.run();
    assert!(
        actions.borrow().iter().any(|action| matches!(
            action,
            CustomPrecisionAction::NoteValueChanged(text) if text.contains('6')
        )),
        "分音符输入应回传 NoteValueChanged，实际 {:?}",
        actions.borrow()
    );

    let (mut harness, actions) = make_harness(model());
    let divisor = harness.get_by(|node| {
        node.role() == egui::Role::TextInput && node.value().as_deref() == Some("1")
    });
    divisor.focus();
    divisor.type_text("2");
    harness.run();
    assert!(
        actions.borrow().iter().any(|action| matches!(
            action,
            CustomPrecisionAction::DivisorChanged(text) if text.contains('2')
        )),
        "除数输入应回传 DivisorChanged，实际 {:?}",
        actions.borrow()
    );
}

/// 点附点按钮回传对应 `DotType`。
#[test]
fn dot_button_dispatches_type() {
    let (mut harness, actions) = make_harness(model());
    harness.get_by_label("双附点").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&CustomPrecisionAction::DotTypeChanged(DotType::Double))
    );
}

/// 错误行出现时可访问（不依赖像素）。
#[test]
fn error_line_is_visible() {
    let mut model = model();
    model.error = Some("三连音数必须大于 0".to_owned());
    let (harness, _) = make_harness(model);
    assert!(harness.query_by_label("三连音数必须大于 0").is_some());
}
