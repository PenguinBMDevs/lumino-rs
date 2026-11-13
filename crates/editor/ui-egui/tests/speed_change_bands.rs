//! 变速对话框的无头交互测试：按钮动作 + 倍率输入回传。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lumino_ui_egui::chrome::{self, ChromeModel, DialogModel, SpeedChangeModel};
use lumino_ui_egui::speed_change::SpeedChangeAction;

fn model() -> SpeedChangeModel {
    SpeedChangeModel {
        titlebar: false,
        title: "变速".to_owned(),
        factor_label: "倍率(S):".to_owned(),
        factor_hint: "如: 0.5 或 1/3".to_owned(),
        factor: "0.5".to_owned(),
        hint: "支持小数 (0.5) 或分数 (1/3)".to_owned(),
        ok: "确定".to_owned(),
        cancel: "取消".to_owned(),
    }
}

fn make_harness(
    model: SpeedChangeModel,
) -> (Harness<'static>, Rc<RefCell<Vec<SpeedChangeAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(360.0, 220.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            let model = ChromeModel {
                dialog: Some(DialogModel::SpeedChange(Box::new(model.clone()))),
                ..ChromeModel::default()
            };
            chrome::show_bands(ui, &model, &mut icons);
            if let Some(action) = lumino_ui_egui::speed_change::take_action(ui.ctx()) {
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
    assert_eq!(actions.borrow().last(), Some(&SpeedChangeAction::Confirm));

    let (mut harness, actions) = make_harness(model());
    harness.get_by_label("取消").click();
    harness.run();
    assert_eq!(actions.borrow().last(), Some(&SpeedChangeAction::Cancel));
}

/// 倍率输入框接受输入并原样回传（解析归生产侧）。
#[test]
fn factor_input_dispatches_text() {
    let (mut harness, actions) = make_harness(model());
    let input = harness.get_by(|node| {
        node.role() == egui::Role::TextInput && node.value().as_deref() == Some("0.5")
    });
    input.focus();
    input.type_text("/3");
    harness.run();

    let dispatched = actions.borrow().clone();
    assert!(
        dispatched.iter().any(|action| matches!(
            action,
            SpeedChangeAction::FactorChanged(text) if text.contains("0.5/3")
        )),
        "倍率输入应回传 FactorChanged，实际 {dispatched:?}"
    );
}

/// 标题、标签与提示进入可访问树。
#[test]
fn labels_are_visible() {
    let (harness, _) = make_harness(model());
    for label in ["变速", "倍率(S):", "支持小数 (0.5) 或分数 (1/3)"] {
        assert!(
            harness.query_by_label(label).is_some(),
            "缺少可访问标签：{label}"
        );
    }
}
