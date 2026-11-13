//! 编辑 / 调色板 / 关于行交互（从 `settings_pages_bands.rs` 拆出）。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use lumino_ui_egui::chrome::{
    self, ChromeModel, DialogModel, SettingsLabels, SettingsPageModel, SettingsPalette,
    SettingsSnapshot, UiField, UiPageModel, UiRow,
};
use lumino_ui_egui::settings::{SettingsAction, take_action};

fn labels() -> SettingsLabels {
    SettingsLabels {
        menu: (0..10).map(|index| format!("M{index}")).collect(),
        title: "General".to_owned(),
        eraser_label: "Eraser".to_owned(),
        eraser_options: vec!["Default".to_owned(), "Direct".to_owned()],
        track_add_label: "Track".to_owned(),
        track_add_options: vec!["Auto".to_owned(), "Stay".to_owned()],
        log_section: "Log".to_owned(),
        log_label: "Keep".to_owned(),
        log_placeholder: "10".to_owned(),
        confirm: "OK".to_owned(),
    }
}

fn harness(model: ChromeModel) -> (Harness<'static>, Rc<RefCell<Vec<SettingsAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            let mut icons = lumino_ui_egui::icons::IconCache::default();
            chrome::show_bands(ui, &model, &mut icons);
            if let Some(action) = take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    (harness, actions)
}

fn click(harness: &mut Harness<'static>, pos: egui::Pos2) {
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(pos));
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    harness.run();
}

fn snapshot(page: SettingsPageModel) -> ChromeModel {
    ChromeModel {
        dialog: Some(DialogModel::Settings(Box::new(SettingsSnapshot {
            palette: SettingsPalette::DARK,
            labels: labels(),
            selected: 0,
            eraser: "Default".to_owned(),
            track_add: "Auto".to_owned(),
            log_retention: "10".to_owned(),
            page,
        }))),
        ..ChromeModel::default()
    }
}

#[test]
fn editing_palette_about_rows_dispatch() {
    let page = UiPageModel {
        title: "编辑".to_owned(),
        rows: vec![
            UiRow::Text {
                field: UiField::HistoryTotalLimit,
                label: "总数".to_owned(),
                hint: String::new(),
                value: "100".to_owned(),
                placeholder: "100".to_owned(),
                width: 80.0,
                suffix: String::new(),
            },
            UiRow::Checkbox {
                field: UiField::AutosaveEnabled,
                label: "自动保存".to_owned(),
                hint: String::new(),
                checked: false,
            },
            UiRow::Swatches {
                colors: vec![
                    egui::Color32::RED,
                    egui::Color32::GREEN,
                    egui::Color32::BLUE,
                ],
            },
            UiRow::AboutLogo {
                dx: 0.0,
                size: 64.0,
                visible: true,
            },
            UiRow::EchoCave {
                text: "回声".to_owned(),
                opacity: 1.0,
            },
        ],
    };
    let (mut harness, actions) = harness(snapshot(SettingsPageModel::Ui(Box::new(page))));

    let hits = lumino_ui_egui::settings::hits(&harness.ctx);
    let field = hits
        .ui_rows
        .iter()
        .find(|(field, _)| *field == UiField::HistoryTotalLimit)
        .map(|(_, rect)| *rect)
        .expect("日志总数输入框");
    click(&mut harness, field.center());
    // 输入框点击只聚焦；键入一个字符才会产生变更。
    harness
        .input_mut()
        .events
        .push(egui::Event::Text("0".to_owned()));
    harness.run();
    assert!(
        matches!(
            actions.borrow().last(),
            Some(SettingsAction::UiText {
                field: UiField::HistoryTotalLimit,
                ..
            })
        ),
        "输入框应回传 UiText"
    );

    let hits = lumino_ui_egui::settings::hits(&harness.ctx);
    let check = hits
        .ui_rows
        .iter()
        .find(|(field, _)| *field == UiField::AutosaveEnabled)
        .map(|(_, rect)| *rect)
        .expect("自动保存开关");
    click(&mut harness, check.center());
    assert_eq!(
        actions.borrow().last(),
        Some(&SettingsAction::UiToggle {
            field: UiField::AutosaveEnabled,
            checked: true,
        })
    );

    let hits = lumino_ui_egui::settings::hits(&harness.ctx);
    let logo = hits
        .ui_rows
        .iter()
        .find(|(field, _)| *field == UiField::AboutLogoClicked)
        .map(|(_, rect)| *rect)
        .expect("关于 logo");
    click(&mut harness, logo.center());
    assert_eq!(
        actions.borrow().last(),
        Some(&SettingsAction::UiButton {
            field: UiField::AboutLogoClicked,
        })
    );

    let hits = lumino_ui_egui::settings::hits(&harness.ctx);
    let echo = hits
        .ui_rows
        .iter()
        .find(|(field, _)| *field == UiField::EchoCaveClicked)
        .map(|(_, rect)| *rect)
        .expect("回声洞");
    click(&mut harness, echo.center());
    assert_eq!(
        actions.borrow().last(),
        Some(&SettingsAction::UiButton {
            field: UiField::EchoCaveClicked,
        })
    );
}
