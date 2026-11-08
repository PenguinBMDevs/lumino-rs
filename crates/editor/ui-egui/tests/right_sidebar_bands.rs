//! 右侧栏「图片转 MIDI」面板的无头交互测试。
//!
//! 验证 band 几何（面板宽 + 48 图标列、内容区扣掉 6px 手柄）与四类动作：
//! 选择图片 / 配置文本 / 调色板下拉 / 转换按钮。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lumino_ui_egui::chrome::right_panel::RightSidebarAction;
use lumino_ui_egui::chrome::{
    self, ChromeModel, I2mControlModel, I2mConvertModel, I2mField, I2mPanelModel, I2mRowModel,
    RightSidebarModel, RightSidebarPanelKind, RightSidebarPanelModel, StatusbarModel,
};

fn statusbar() -> StatusbarModel {
    StatusbarModel {
        left: "就绪".to_owned(),
        left_accent: true,
        cpu_percent: None,
        memory_mb: 0.0,
        fps: None,
        right: String::new(),
    }
}

fn panel_model(convert: bool) -> I2mPanelModel {
    I2mPanelModel {
        title: "图片转 MIDI".to_owned(),
        select_image: "选择图片文件".to_owned(),
        selected_hint: convert.then(|| "选择了一个图片文件".to_owned()),
        config_title: "转换参数".to_owned(),
        rows: vec![
            I2mRowModel {
                label: "Key 范围".to_owned(),
                control: I2mControlModel::KeyRange {
                    start_field: I2mField::StartKey,
                    start_text: "21".to_owned(),
                    end_field: I2mField::EndKey,
                    end_text: "108".to_owned(),
                },
            },
            I2mRowModel {
                label: "目标高度".to_owned(),
                control: I2mControlModel::Input {
                    field: I2mField::TargetHeight,
                    text: String::new(),
                },
            },
            I2mRowModel {
                label: "每像素 tick".to_owned(),
                control: I2mControlModel::Input {
                    field: I2mField::TicksPerPixel,
                    text: "1".to_owned(),
                },
            },
            I2mRowModel {
                label: "颜色数".to_owned(),
                control: I2mControlModel::Input {
                    field: I2mField::ColorCount,
                    text: "16".to_owned(),
                },
            },
            I2mRowModel {
                label: "调色板".to_owned(),
                control: I2mControlModel::Palette {
                    options: vec![
                        "K-Means++".to_owned(),
                        "K-Means".to_owned(),
                        "Octree".to_owned(),
                    ],
                    selected: 0,
                },
            },
        ],
        convert: convert.then(|| I2mConvertModel {
            label: "转换为 MIDI".to_owned(),
            enabled: true,
        }),
    }
}

fn chrome(convert: bool) -> ChromeModel {
    ChromeModel {
        right_sidebar: Some(RightSidebarModel {
            panel_visible: true,
            panel_width: 200.0,
            active: Some(RightSidebarPanelKind::ImageToMidi),
            top_inset: 102.0,
            resizing: false,
            content: Some(RightSidebarPanelModel::ImageToMidi(panel_model(convert))),
        }),
        statusbar: statusbar(),
        ..ChromeModel::default()
    }
}

fn harness(model: ChromeModel) -> (Harness<'static>, Rc<RefCell<Vec<RightSidebarAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 900.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            chrome::show_bands(ui, &model, &mut icons);
            if let Some(action) = chrome::right_panel::take_action(ui.ctx()) {
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

/// 面板几何：内容区 = 面板宽 200 − 6 手柄，右边缘贴住 48px 图标列。
#[test]
fn panel_band_geometry_matches_production() {
    let (harness, _) = harness(chrome(true));
    let rect =
        lumino_ui_egui::chrome::right_panel::panel_rect(&harness.ctx).expect("应记录面板内容矩形");
    // 视口 1600：图标列 48 → 内容右边缘 1552；内容宽 194。
    assert!((rect.right() - 1552.0).abs() < 0.5, "内容右边缘 {rect:?}");
    assert!((rect.width() - 194.0).abs() < 0.5, "内容宽度 {rect:?}");
    assert!((rect.top() - 102.0).abs() < 0.5, "顶边 {rect:?}");
    assert!((rect.bottom() - 880.0).abs() < 0.5, "底边 {rect:?}");

    // 内容区整块占用；左侧 6px 拖拽手柄不占用（iced 继续处理缩放拖拽）。
    let blocked = chrome::menu::take_blocked(&harness.ctx);
    assert!(blocked.contains(&rect), "内容区必须进入占用列表");
    let handle = egui::pos2(rect.left() - 3.0, rect.center().y);
    assert!(
        !blocked.iter().any(|block| block.contains(handle)),
        "拖拽手柄必须留给 iced"
    );
}

/// 选择图片按钮 → `SelectImageFile`。
#[test]
fn select_image_button_dispatches() {
    let (mut harness, actions) = harness(chrome(false));
    let button = lumino_ui_egui::chrome::right_panel::button_rect(&harness.ctx, "选择图片文件")
        .expect("选择图片按钮");
    click(&mut harness, button.center());
    assert_eq!(
        actions.borrow().last(),
        Some(&RightSidebarAction::SelectImageFile)
    );
}

/// 配置输入框：键入后回传字段 + 文本。
#[test]
fn config_input_dispatches_field_and_text() {
    let (mut harness, actions) = harness(chrome(false));
    let field =
        lumino_ui_egui::chrome::right_panel::field_rect(&harness.ctx, I2mField::TargetHeight)
            .expect("目标高度输入框");
    click(&mut harness, field.center());
    harness
        .input_mut()
        .events
        .push(egui::Event::Text("24".to_owned()));
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&RightSidebarAction::I2mConfigTextChanged {
            field: I2mField::TargetHeight,
            text: "24".to_owned(),
        })
    );
}

/// 调色板下拉：选第三项 → `I2mPaletteChanged(2)`。
#[test]
fn palette_combo_dispatches_index() {
    let (mut harness, actions) = harness(chrome(false));
    let fields =
        lumino_ui_egui::chrome::right_panel::field_rect(&harness.ctx, I2mField::ColorCount);
    assert!(fields.is_some(), "前置：字段矩形已记录");
    // 下拉框矩形没有单独暴露：用面板右侧 118 宽、最后一行中心推算。
    let panel = lumino_ui_egui::chrome::right_panel::panel_rect(&harness.ctx).expect("面板");
    let y = panel.top() + 8.0 + 20.0 + 8.0 + 28.0 + 8.0 + 18.0 + 4.0 + 4.0 * 26.0 + 11.0;
    click(&mut harness, egui::pos2(panel.right() - 8.0 - 59.0, y));
    harness.get_by_label("Octree").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&RightSidebarAction::I2mPaletteChanged(2))
    );
}

/// 转换按钮只在已选择文件时出现；点击回传 `ConvertClicked`。
#[test]
fn convert_button_only_when_file_selected() {
    let (first, _) = harness(chrome(false));
    assert!(
        lumino_ui_egui::chrome::right_panel::button_rect(&first.ctx, "转换为 MIDI").is_none(),
        "未选择文件时不应有转换按钮"
    );
    let (mut harness, actions) = harness(chrome(true));
    let button = lumino_ui_egui::chrome::right_panel::button_rect(&harness.ctx, "转换为 MIDI")
        .expect("转换按钮");
    click(&mut harness, button.center());
    assert_eq!(
        actions.borrow().last(),
        Some(&RightSidebarAction::ConvertClicked)
    );
}
