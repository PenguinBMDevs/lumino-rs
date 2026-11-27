//! 「端口音色库未生效」提示窗的无头交互测试（REND-005）。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lumino_ui_egui::chrome::{self, ChromeModel, DialogModel, PortSoundfontsUnsupportedModel};
use lumino_ui_egui::port_soundfonts_unsupported::PortSoundfontsUnsupportedAction;

fn model() -> PortSoundfontsUnsupportedModel {
    PortSoundfontsUnsupportedModel {
        titlebar: false,
        title: "端口音色库未生效".to_owned(),
        message: "当前后端「LGS (GPU)」不支持按端口音色库。".to_owned(),
        detail: "已保存 2 个端口的映射；当前播放与导出将使用全局音色库（降级运行）。".to_owned(),
        acknowledge: "继续（使用全局音色库）".to_owned(),
        open_settings: "打开合成器设置".to_owned(),
    }
}

fn make_harness(
    model: PortSoundfontsUnsupportedModel,
) -> (
    Harness<'static>,
    Rc<RefCell<Vec<PortSoundfontsUnsupportedAction>>>,
) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(500.0, 260.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            let model = ChromeModel {
                dialog: Some(DialogModel::PortSoundfontsUnsupported(Box::new(
                    model.clone(),
                ))),
                ..ChromeModel::default()
            };
            chrome::show_bands(ui, &model, &mut icons);
            if let Some(action) = lumino_ui_egui::port_soundfonts_unsupported::take_action(ui.ctx())
            {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    (harness, actions)
}

/// 文案与两个按钮进入可访问树。
#[test]
fn texts_and_buttons_are_visible() {
    let (harness, _) = make_harness(model());
    for label in [
        "端口音色库未生效",
        "当前后端「LGS (GPU)」不支持按端口音色库。",
        "继续（使用全局音色库）",
        "打开合成器设置",
    ] {
        assert!(
            harness.query_by_label(label).is_some(),
            "缺少可访问标签：{label}"
        );
    }
}

/// 两个按钮分别回传 Acknowledge / OpenSettings。
#[test]
fn buttons_dispatch_actions() {
    let (mut harness, actions) = make_harness(model());
    harness.get_by_label("继续（使用全局音色库）").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&PortSoundfontsUnsupportedAction::Acknowledge)
    );

    let (mut harness, actions) = make_harness(model());
    harness.get_by_label("打开合成器设置").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&PortSoundfontsUnsupportedAction::OpenSettings)
    );
}
