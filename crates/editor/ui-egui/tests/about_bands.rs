//! 关于窗口的无头渲染测试：品牌行与版本号进入可访问树。

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lumino_ui_egui::chrome::{self, AboutModel, ChromeModel, DialogModel};
use lumino_ui_egui::icons::IconCache;

fn make_harness(model: AboutModel) -> Harness<'static> {
    let mut icons = IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(520.0, 220.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            let model = ChromeModel {
                dialog: Some(DialogModel::About(Box::new(model.clone()))),
                ..ChromeModel::default()
            };
            chrome::show_bands(ui, &model, &mut icons);
        });
    harness.run();
    harness
}

#[test]
fn brand_and_version_are_visible() {
    let harness = make_harness(AboutModel {
        titlebar: false,
        title: "Lumino - 新一代MIDI编辑器".to_owned(),
        version: "版本 0.2.1".to_owned(),
    });

    for label in ["Lumino - 新一代MIDI编辑器", "版本 0.2.1"] {
        assert!(
            harness.query_by_label(label).is_some(),
            "缺少可访问标签：{label}"
        );
    }
}
