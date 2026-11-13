//! 撤销历史对话框的无头交互测试：行跳转 + 关闭。
//!
//! 窗口按真实栅格（440×560）渲染；标题栏留白由 `titlebar = false` 关闭，
//! 便于在整窗内定位行与按钮。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lumino_ui_egui::chrome::{self, ChromeModel, DialogModel, HistoryModel, HistoryRowModel};
use lumino_ui_egui::history::HistoryAction;

fn model(current: usize) -> HistoryModel {
    let mut rows = vec![HistoryRowModel {
        index: 0,
        label: "初始状态".to_owned(),
        current: current == 0,
        future: false,
    }];
    for (index, label) in ["新建音符", "移动音符", "删除音符"].iter().enumerate() {
        let step = index + 1;
        rows.push(HistoryRowModel {
            index: step,
            label: (*label).to_owned(),
            current: current == step,
            future: step > current,
        });
    }
    HistoryModel {
        titlebar: false,
        title: "撤销历史".to_owned(),
        hint: "点击步骤回到该步完成后的状态；初始状态为 0".to_owned(),
        rows,
        counter: format!("{current} / 3"),
        close: "关闭".to_owned(),
    }
}

fn harness(model: HistoryModel) -> (Harness<'static>, Rc<RefCell<Vec<HistoryAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(440.0, 560.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            let model = ChromeModel {
                dialog: Some(DialogModel::History(Box::new(model.clone()))),
                ..ChromeModel::default()
            };
            chrome::show_bands(ui, &model, &mut icons);
            if let Some(action) = lumino_ui_egui::history::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    (harness, actions)
}

/// 点击任意一行回传该行的 `Jump(index)`（含未来步骤）。
#[test]
fn row_click_dispatches_jump() {
    let (mut harness, actions) = harness(model(1));

    // 非当前行没有游标标记，标签前有两个空格（与生产 旧版 视图一致）。
    harness.get_by_label("  移动音符").click();
    harness.run();
    assert_eq!(actions.borrow().last(), Some(&HistoryAction::Jump(2)));

    // 初始状态行也能点（回到 0）。
    harness.get_by_label("  初始状态").click();
    harness.run();
    assert_eq!(actions.borrow().last(), Some(&HistoryAction::Jump(0)));
}

/// 关闭按钮回传 `Close`；计数与提示进入可访问树。
#[test]
fn close_button_and_labels() {
    let (mut harness, actions) = harness(model(2));
    assert!(harness.query_by_label_contains("2 / 3").is_some());
    assert!(harness.query_by_label_contains("点击步骤").is_some());

    harness.get_by_label("关闭").click();
    harness.run();
    assert_eq!(actions.borrow().last(), Some(&HistoryAction::Close));
}
