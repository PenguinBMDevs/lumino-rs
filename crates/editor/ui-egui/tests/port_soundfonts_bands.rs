//! 端口音色库对话框的无头交互测试（REND-005 P4）。
//!
//! 按真实栅格（560 宽）渲染，验证端口行可见、选择/清除回传正确端口、
//! 底部按钮回传确认/取消。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lumino_ui_egui::chrome::{
    self, ChromeModel, DialogModel, PortSoundfontsModel, PortSoundfontsRowModel,
};
use lumino_ui_egui::port_soundfonts::PortSoundfontsAction;

fn model() -> PortSoundfontsModel {
    PortSoundfontsModel {
        titlebar: false,
        title: "端口音色库".to_owned(),
        hint: "为每个 MIDI 端口绑定音色库".to_owned(),
        rows: vec![
            PortSoundfontsRowModel {
                port: 0,
                label: "端口 0（全局，默认，可覆盖）".to_owned(),
                path: String::new(),
                path_hint: "未配置：回退全局音色库".to_owned(),
                extra: None,
                browse: "选择…".to_owned(),
                clear: "清除".to_owned(),
            },
            PortSoundfontsRowModel {
                port: 1,
                label: "端口 1".to_owned(),
                path: "D:/音色库/port1.sf2".to_owned(),
                path_hint: "未配置：回退全局音色库".to_owned(),
                extra: Some("另有 1 个回退库（当前 UI 只显示首个）".to_owned()),
                browse: "选择…".to_owned(),
                clear: "清除".to_owned(),
            },
        ],
        ok: "确定".to_owned(),
        cancel: "取消".to_owned(),
    }
}

fn make_harness(
    model: PortSoundfontsModel,
) -> (Harness<'static>, Rc<RefCell<Vec<PortSoundfontsAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(560.0, 520.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            let model = ChromeModel {
                dialog: Some(DialogModel::PortSoundfonts(Box::new(model.clone()))),
                ..ChromeModel::default()
            };
            chrome::show_bands(ui, &model, &mut icons);
            if let Some(action) = lumino_ui_egui::port_soundfonts::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    (harness, actions)
}

/// 端口行与说明进入可访问树。
#[test]
fn rows_and_hints_are_visible() {
    let (harness, _) = make_harness(model());
    for label in [
        "端口音色库",
        "端口 0（全局，默认，可覆盖）",
        "端口 1",
        "未配置：回退全局音色库",
        "D:/音色库/port1.sf2",
        "另有 1 个回退库（当前 UI 只显示首个）",
    ] {
        assert!(
            harness.query_by_label(label).is_some(),
            "缺少可访问标签：{label}"
        );
    }
}

/// 第二行的「选择…」/「清除」必须回传端口 1，而不是端口 0。
#[test]
fn row_buttons_dispatch_their_own_port() {
    let (mut harness, actions) = make_harness(model());

    harness
        .get_all_by_label("选择…")
        .nth(1)
        .expect("第二行应有选择按钮")
        .click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&PortSoundfontsAction::Browse(1)),
        "选择按钮必须回传所在端口"
    );

    harness
        .get_all_by_label("清除")
        .nth(1)
        .expect("第二行应有清除按钮")
        .click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&PortSoundfontsAction::Clear(1)),
        "清除按钮必须回传所在端口"
    );
}

/// 确定 / 取消按钮各自回传对应动作。
#[test]
fn bottom_buttons_dispatch_confirm_and_cancel() {
    let (mut harness, actions) = make_harness(model());
    harness.get_by_label("确定").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&PortSoundfontsAction::Confirm)
    );

    let (mut harness, actions) = make_harness(model());
    harness.get_by_label("取消").click();
    harness.run();
    assert_eq!(actions.borrow().last(), Some(&PortSoundfontsAction::Cancel));
}
