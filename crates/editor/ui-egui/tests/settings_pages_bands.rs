//! 设置对话框交互路由：菜单 / 确认 / 洋葱皮下拉 / 兼容性开关。
//!
//! 像素基线仍由 `settings_bands.rs` 负责；这里只验证「点击 → SettingsAction」。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lumino_ui_egui::chrome::{
    self, AudioControlModel, AudioField, AudioPageModel, AudioRowModel, ChromeModel, CompatLine,
    CompatibilityPageModel, DialogModel, OnionSkinPageModel, SettingsLabels, SettingsPageModel,
    SettingsPalette, SettingsSnapshot, ThemeCardModel, UiField, UiPageModel, UiRow,
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

/// 菜单：行高 52、容器内边距 20、外边距 16 → 行矩形由页面记录。
#[test]
fn menu_click_dispatches_page_index() {
    let (mut harness, actions) = harness(snapshot(SettingsPageModel::General));
    let rows = lumino_ui_egui::settings::hits(&harness.ctx).menu_rows;
    assert_eq!(rows.len(), 10, "应记录 10 个菜单行");
    click(&mut harness, rows[3].center());
    assert_eq!(
        actions.borrow().last(),
        Some(&SettingsAction::MenuSelected(3))
    );
}

/// 确认按钮：卡片外右下角 100×34，点击回传 Confirm。
#[test]
fn confirm_button_dispatches_action() {
    let (mut harness, actions) = harness(snapshot(SettingsPageModel::General));
    // 1200×800：inner.right=1184 → 按钮 x 1084..1184，y 750..784。
    click(&mut harness, egui::pos2(1134.0, 767.0));
    assert_eq!(actions.borrow().last(), Some(&SettingsAction::Confirm));
}

/// 洋葱皮页：下拉选择 CPU → `OnionSkinBackend(Cpu)`。
#[test]
fn onion_combo_dispatches_backend() {
    let page = OnionSkinPageModel {
        title: "洋葱皮设置".to_owned(),
        label: "洋葱皮渲染方式".to_owned(),
        options: vec!["GPU".to_owned(), "CPU".to_owned()],
        selected: 0,
        locked: false,
        locked_value: "GPU".to_owned(),
        locked_hint: String::new(),
        hint: "CPU 模式会禁用部分视图".to_owned(),
    };
    let (mut harness, actions) = harness(snapshot(SettingsPageModel::OnionSkin(page)));
    let combo = lumino_ui_egui::settings::hits(&harness.ctx)
        .onion_combo
        .expect("应记录洋葱皮下拉矩形");
    click(&mut harness, combo.center());
    // 弹层选项是可访问节点：直接按文本点击，避免依赖弹层像素偏移。
    harness.get_by_label("CPU").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&SettingsAction::OnionSkinBackend(
            lumino_core::storage::config::OnionSkinBackend::Cpu
        ))
    );
}

fn row(field: AudioField, control: AudioControlModel) -> AudioRowModel {
    AudioRowModel {
        field,
        label: field_label(field),
        hint: String::new(),
        control,
        width: 160.0,
    }
}

fn field_label(field: AudioField) -> String {
    format!("{field:?}")
}

/// 音频页：下拉 / 开关 / 按钮都回传字段级动作。
#[test]
fn audio_page_dispatches_field_actions() {
    let page = AudioPageModel {
        title: "音频".to_owned(),
        rows: vec![
            row(
                AudioField::OutputType,
                AudioControlModel::Combo {
                    options: vec![
                        "Built-in".to_owned(),
                        "KDMAPI".to_owned(),
                        "System".to_owned(),
                    ],
                    selected: 0,
                },
            ),
            row(
                AudioField::XSynthGate,
                AudioControlModel::Checkbox { checked: false },
            ),
            row(AudioField::BrowseSoundfont, AudioControlModel::Button),
        ],
    };
    let (mut harness, actions) = harness(snapshot(SettingsPageModel::Audio(page)));

    let hits = lumino_ui_egui::settings::hits(&harness.ctx);
    let gate = hits
        .audio_rows
        .iter()
        .find(|(field, _)| *field == AudioField::XSynthGate)
        .map(|(_, rect)| *rect)
        .expect("开关矩形");
    click(&mut harness, gate.center());
    assert_eq!(
        actions.borrow().last(),
        Some(&SettingsAction::AudioToggle {
            field: AudioField::XSynthGate,
            checked: true,
        })
    );

    let hits = lumino_ui_egui::settings::hits(&harness.ctx);
    let browse = hits
        .audio_rows
        .iter()
        .find(|(field, _)| *field == AudioField::BrowseSoundfont)
        .map(|(_, rect)| *rect)
        .expect("按钮矩形");
    click(&mut harness, browse.center());
    assert_eq!(
        actions.borrow().last(),
        Some(&SettingsAction::AudioButton {
            field: AudioField::BrowseSoundfont,
        })
    );

    let hits = lumino_ui_egui::settings::hits(&harness.ctx);
    let combo = hits
        .audio_rows
        .iter()
        .find(|(field, _)| *field == AudioField::OutputType)
        .map(|(_, rect)| *rect)
        .expect("下拉矩形");
    click(&mut harness, combo.center());
    harness.get_by_label("System").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&SettingsAction::AudioCombo {
            field: AudioField::OutputType,
            index: 2,
        })
    );
}

fn theme_card(display: &str, value: &str, selected: bool) -> ThemeCardModel {
    let color = egui::Color32::from_rgb(0x22, 0x22, 0x22);
    ThemeCardModel {
        display: display.to_owned(),
        value: value.to_owned(),
        selected,
        bg: color,
        title: egui::Color32::WHITE,
        control: color,
        bar_primary: color,
        bar_secondary: color,
        accent: color,
        line: color,
        line_soft: color,
    }
}

/// 界面页：主题卡片 / 开关 / 下拉回传字段动作。
#[test]
fn ui_page_dispatches_theme_and_fields() {
    let page = UiPageModel {
        title: "界面".to_owned(),
        rows: vec![
            UiRow::ThemeCards {
                label: "主题".to_owned(),
                cards: vec![
                    theme_card("Dark", "Dark", true),
                    theme_card("Light", "Light", false),
                ],
            },
            UiRow::Checkbox {
                field: UiField::Enable256Key,
                label: "256".to_owned(),
                hint: String::new(),
                checked: false,
            },
            UiRow::Combo {
                field: UiField::Language,
                label: "Language".to_owned(),
                hint: String::new(),
                options: vec!["System".to_owned(), "中文".to_owned(), "English".to_owned()],
                selected: 0,
                width: 200.0,
            },
        ],
    };
    let (mut harness, actions) = harness(snapshot(SettingsPageModel::Ui(Box::new(page))));

    let hits = lumino_ui_egui::settings::hits(&harness.ctx);
    let light = hits
        .theme_cards
        .iter()
        .find(|(value, _)| value == "Light")
        .map(|(_, rect)| *rect)
        .expect("Light 卡片");
    click(&mut harness, light.center());
    assert_eq!(
        actions.borrow().last(),
        Some(&SettingsAction::UiTheme("Light".to_owned()))
    );

    let hits = lumino_ui_egui::settings::hits(&harness.ctx);
    let check = hits
        .ui_rows
        .iter()
        .find(|(field, _)| *field == UiField::Enable256Key)
        .map(|(_, rect)| *rect)
        .expect("256 开关");
    click(&mut harness, check.center());
    assert_eq!(
        actions.borrow().last(),
        Some(&SettingsAction::UiToggle {
            field: UiField::Enable256Key,
            checked: true,
        })
    );

    let hits = lumino_ui_egui::settings::hits(&harness.ctx);
    let combo = hits
        .ui_rows
        .iter()
        .find(|(field, _)| *field == UiField::Language)
        .map(|(_, rect)| *rect)
        .expect("语言下拉");
    click(&mut harness, combo.center());
    harness.get_by_label("English").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&SettingsAction::UiCombo {
            field: UiField::Language,
            index: 2,
        })
    );
}

/// 兼容性页：检查按钮、两个开关。
#[test]
fn compatibility_buttons_and_toggles_dispatch() {
    let page = CompatibilityPageModel {
        title: "兼容性".to_owned(),
        check_label: "检查".to_owned(),
        check_enabled: true,
        copy_label: "复制诊断".to_owned(),
        copy_enabled: false,
        result_lines: vec![CompatLine {
            text: "尚未检测".to_owned(),
            dim: true,
        }],
        check_on_startup: false,
        check_on_startup_label: "启动时检查".to_owned(),
        check_on_startup_hint: "提示".to_owned(),
        show_warning: true,
        warning_label: "显示警告".to_owned(),
        warning_hint: "提示".to_owned(),
        domino: false,
        domino_enabled: true,
        domino_label: "Domino".to_owned(),
        domino_hint: "提示".to_owned(),
    };
    let (mut harness, actions) = harness(snapshot(SettingsPageModel::Compatibility(page)));

    let hits = lumino_ui_egui::settings::hits(&harness.ctx);
    click(&mut harness, hits.compat_check.expect("检查按钮").center());
    assert_eq!(actions.borrow().last(), Some(&SettingsAction::RunGpuCheck));

    let hits = lumino_ui_egui::settings::hits(&harness.ctx);
    let startup = hits
        .compat_checks
        .iter()
        .find(|(label, _)| label == "启动时检查")
        .map(|(_, rect)| *rect)
        .expect("启动检查开关");
    click(&mut harness, startup.center());
    assert_eq!(
        actions.borrow().last(),
        Some(&SettingsAction::GpuCheckOnStartup(true))
    );

    let hits = lumino_ui_egui::settings::hits(&harness.ctx);
    let domino = hits
        .compat_checks
        .iter()
        .find(|(label, _)| label == "Domino")
        .map(|(_, rect)| *rect)
        .expect("Domino 开关");
    click(&mut harness, domino.center());
    assert_eq!(
        actions.borrow().last(),
        Some(&SettingsAction::DominoClipboard(true))
    );
}
