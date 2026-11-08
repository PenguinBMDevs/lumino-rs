//! 批量编辑对话框的无头交互测试：按钮动作 + 输入回传。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lumino_ui_egui::batch_edit::BatchEditAction;
use lumino_ui_egui::chrome::{
    self, BatchEditFieldModel, BatchEditModel, BatchEditRowModel, ChromeModel, DialogModel,
};

fn model() -> BatchEditModel {
    BatchEditModel {
        titlebar: false,
        title: "批量编辑".to_owned(),
        settings_title: "设置".to_owned(),
        rows: vec![
            BatchEditRowModel {
                field: BatchEditFieldModel::Velocity,
                label: "音符力度:".to_owned(),
                placeholder: "如: +20".to_owned(),
                value: "10".to_owned(),
            },
            BatchEditRowModel {
                field: BatchEditFieldModel::Gate,
                label: "音符长度:".to_owned(),
                placeholder: "如: *1.5".to_owned(),
                value: "1.2".to_owned(),
            },
            BatchEditRowModel {
                field: BatchEditFieldModel::Key,
                label: "音符key位置:".to_owned(),
                placeholder: "如: 60".to_owned(),
                value: "60".to_owned(),
            },
            BatchEditRowModel {
                field: BatchEditFieldModel::Tick,
                label: "音符tick位置:".to_owned(),
                placeholder: "如: 1000".to_owned(),
                value: "1000".to_owned(),
            },
        ],
        format_title: "格式".to_owned(),
        format_hints: vec![
            "支持以下运算符号:".to_owned(),
            "+N  增加 N        -N  减少 N".to_owned(),
            "*N  乘以 N        /N  除以 N".to_owned(),
            "无符号数字表示直接设置为该值".to_owned(),
        ],
        ok: "确定".to_owned(),
        cancel: "取消".to_owned(),
    }
}

fn make_harness(model: BatchEditModel) -> (Harness<'static>, Rc<RefCell<Vec<BatchEditAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(460.0, 660.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            let model = ChromeModel {
                dialog: Some(DialogModel::BatchEdit(Box::new(model.clone()))),
                ..ChromeModel::default()
            };
            chrome::show_bands(ui, &model, &mut icons);
            if let Some(action) = lumino_ui_egui::batch_edit::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    (harness, actions)
}

#[test]
fn buttons_dispatch_confirm_and_cancel() {
    let (mut harness, actions) = make_harness(model());
    harness.get_by_label("确定").click();
    harness.run();
    assert_eq!(actions.borrow().last(), Some(&BatchEditAction::Confirm));

    let (mut harness, actions) = make_harness(model());
    harness.get_by_label("取消").click();
    harness.run();
    assert_eq!(actions.borrow().last(), Some(&BatchEditAction::Cancel));
}

/// 力度输入框（按当前值定位）接受输入并回传 `InputChanged(Velocity, …)`。
#[test]
fn input_dispatches_field_and_text() {
    let (mut harness, actions) = make_harness(model());
    let input = harness.get_by(|node| {
        node.role() == egui::Role::TextInput && node.value().as_deref() == Some("10")
    });
    input.focus();
    input.type_text("5");
    harness.run();

    let dispatched = actions.borrow().clone();
    assert!(
        dispatched.iter().any(|action| matches!(
            action,
            BatchEditAction::InputChanged(BatchEditFieldModel::Velocity, text) if text.contains('5')
        )),
        "力度输入应回传 InputChanged(Velocity)，实际 {dispatched:?}"
    );
}

/// 小节标题与格式说明进入可访问树。
#[test]
fn labels_are_visible() {
    let (harness, _) = make_harness(model());
    for label in [
        "批量编辑",
        "设置",
        "格式",
        "音符力度:",
        "无符号数字表示直接设置为该值",
    ] {
        assert!(
            harness.query_by_label(label).is_some(),
            "缺少可访问标签：{label}"
        );
    }
}
