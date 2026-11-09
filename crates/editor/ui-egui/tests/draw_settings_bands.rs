//! 音符画设置对话框：无头测试（占位文案 + 关闭）。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lumino_ui_egui::chrome::{self, ChromeModel, DialogModel, DrawSettingsModel};
use lumino_ui_egui::draw_settings::DrawSettingsAction;
use lumino_ui_egui::icons::IconCache;

fn model() -> DrawSettingsModel {
    DrawSettingsModel {
        titlebar: false,
        title: "音符画设置".to_owned(),
        placeholder: "该页面的设置项正在逐步接入：当前仅在悬浮工具条上提供常用参数。".to_owned(),
        close: "关闭".to_owned(),
    }
}

#[test]
fn placeholder_and_close_are_visible_and_dispatch() {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(380.0, 220.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            let model = ChromeModel {
                dialog: Some(DialogModel::DrawSettings(Box::new(model()))),
                ..ChromeModel::default()
            };
            chrome::show_bands(ui, &model, &mut icons);
            if let Some(action) = lumino_ui_egui::draw_settings::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();

    assert!(harness.query_by_label("音符画设置").is_some());
    assert!(
        harness
            .query_by_label("该页面的设置项正在逐步接入：当前仅在悬浮工具条上提供常用参数。")
            .is_some()
    );
    harness.get_by_label("关闭").click();
    harness.run();
    assert_eq!(actions.borrow().last(), Some(&DrawSettingsAction::Close));
}
