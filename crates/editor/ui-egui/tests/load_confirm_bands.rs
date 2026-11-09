//! 载入确认对话框的无头测试：内容 + 两个按钮。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lumino_ui_egui::chrome::{self, ChromeModel, DialogModel, LoadConfirmModel};
use lumino_ui_egui::icons::IconCache;
use lumino_ui_egui::load_confirm::LoadConfirmAction;

fn model() -> LoadConfirmModel {
    LoadConfirmModel {
        titlebar: false,
        file_line: "文件: huge.lmpj".to_owned(),
        size_line: "大小: 128.4 MB".to_owned(),
        path_line: "路径: D:/projects/huge.lmpj".to_owned(),
        hint: "此文件较大，加载可能需要一些时间…".to_owned(),
        cancel: "取消".to_owned(),
        confirm: "加载".to_owned(),
    }
}

fn make_harness(
    model: LoadConfirmModel,
) -> (Harness<'static>, Rc<RefCell<Vec<LoadConfirmAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(420.0, 260.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            let model = ChromeModel {
                dialog: Some(DialogModel::LoadConfirm(Box::new(model.clone()))),
                ..ChromeModel::default()
            };
            chrome::show_bands(ui, &model, &mut icons);
            if let Some(action) = lumino_ui_egui::load_confirm::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    (harness, actions)
}

#[test]
fn lines_and_buttons_are_visible() {
    let (harness, _) = make_harness(model());
    for label in [
        "文件: huge.lmpj",
        "大小: 128.4 MB",
        "路径: D:/projects/huge.lmpj",
        "此文件较大，加载可能需要一些时间…",
        "取消",
        "加载",
    ] {
        assert!(
            harness.query_by_label(label).is_some(),
            "缺少可访问标签：{label}"
        );
    }
}

#[test]
fn buttons_dispatch_confirm_and_cancel() {
    let (mut harness, actions) = make_harness(model());
    harness.get_by_label("加载").click();
    harness.run();
    assert_eq!(actions.borrow().last(), Some(&LoadConfirmAction::Confirm));

    let (mut harness, actions) = make_harness(model());
    harness.get_by_label("取消").click();
    harness.run();
    assert_eq!(actions.borrow().last(), Some(&LoadConfirmAction::Cancel));
}
