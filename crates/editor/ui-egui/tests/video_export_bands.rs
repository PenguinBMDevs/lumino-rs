//! 视频导出面板（egui）无头测试：分区内容 / 下拉 / 滑杆 / 复选 / 文本 / 按钮。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lumino_ui_egui::chrome::{
    TextPairCell, VideoButton, VideoButtonFill, VideoExportModel, VideoExportPanelModel,
    VideoPreviewModel, VideoRow, VideoSection,
};
use lumino_ui_egui::video_export::{self, VideoExportAction};

fn model() -> VideoExportModel {
    VideoExportModel {
        title: "视频导出".to_owned(),
        sections: vec![
            VideoSection {
                title: "MIDI 数据源".to_owned(),
                rows: vec![
                    VideoRow::Text {
                        label: String::new(),
                        label_width: 0.0,
                        value: "C:/a.mid".to_owned(),
                        hint: String::new(),
                        width: 0.0,
                        id: 14,
                    },
                    VideoRow::Buttons {
                        buttons: vec![VideoButton {
                            label: "浏览...".to_owned(),
                            id: 15,
                            fill: VideoButtonFill::Primary,
                            width: 0.0,
                        }],
                    },
                ],
            },
            VideoSection {
                title: "渲染设置".to_owned(),
                rows: vec![
                    VideoRow::Pick {
                        label: "渲染格式:".to_owned(),
                        label_width: 100.0,
                        options: vec!["MP4".to_owned(), "MOV".to_owned()],
                        selected: 0,
                        width: 200.0,
                        id: 1,
                    },
                    VideoRow::Slider {
                        label: "下落速度:".to_owned(),
                        label_width: 100.0,
                        value: 1.0,
                        min: 0.1,
                        max: 10.0,
                        step: 0.1,
                        slider_width: 200.0,
                        value_text: "1.0x".to_owned(),
                        id: 6,
                    },
                    VideoRow::Toggle {
                        label: "3D 音符".to_owned(),
                        checked: false,
                        id: 9,
                    },
                    VideoRow::Text {
                        label: "分辨率:".to_owned(),
                        label_width: 100.0,
                        value: "1920".to_owned(),
                        hint: "1920".to_owned(),
                        width: 80.0,
                        id: 10,
                    },
                    VideoRow::Label {
                        text: "提示文字".to_owned(),
                        size: 12.0,
                        muted: true,
                    },
                ],
            },
        ],
        preview: Some(VideoPreviewModel {
            label: "等待渲染...".to_owned(),
            image_size: None,
        }),
        buttons: vec![
            VideoButton {
                label: "关闭".to_owned(),
                id: 17,
                fill: VideoButtonFill::Neutral,
                width: 100.0,
            },
            VideoButton {
                label: "开始导出".to_owned(),
                id: 18,
                fill: VideoButtonFill::Primary,
                width: 120.0,
            },
        ],
    }
}

fn make_harness() -> (Harness<'static>, Rc<RefCell<Vec<VideoExportAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(760.0, 900.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            let panel = VideoExportPanelModel {
                rect: egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(760.0, 900.0)),
                content: model(),
            };
            video_export::show_panel(ui, &panel);
            if let Some(action) = video_export::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    (harness, actions)
}

#[test]
fn sections_and_controls_are_visible() {
    let (harness, _) = make_harness();
    for label in [
        "视频导出",
        "MIDI 数据源",
        "浏览...",
        "渲染设置",
        "渲染格式:",
        "下落速度:",
        "1.0x",
        "3D 音符",
        "提示文字",
        "等待渲染...",
        "关闭",
        "开始导出",
    ] {
        assert!(
            harness.query_by_label(label).is_some(),
            "缺少可访问标签：{label}"
        );
    }
}

#[test]
fn buttons_dispatch_ids() {
    let (mut harness, actions) = make_harness();
    harness.get_by_label("开始导出").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&VideoExportAction::Button { id: 18 })
    );

    let (mut harness, actions) = make_harness();
    harness.get_by_label("浏览...").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&VideoExportAction::Button { id: 15 })
    );
}

#[test]
fn toggle_dispatch() {
    let (mut harness, actions) = make_harness();
    harness.get_by_label("3D 音符").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&VideoExportAction::Toggle { id: 9, value: true })
    );
}

#[test]
fn pick_and_text_dispatch() {
    let (mut harness, actions) = make_harness();
    harness
        .get_by(|node| node.role() == egui::Role::ComboBox)
        .click();
    harness.run();
    harness.run();
    harness
        .get_all_by_label("MOV")
        .last()
        .expect("下拉应有 MOV")
        .click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&VideoExportAction::Pick { id: 1, index: 1 })
    );

    let (mut harness, actions) = make_harness();
    let input = harness.get_by(|node| {
        node.role() == egui::Role::TextInput && node.value().as_deref() == Some("1920")
    });
    input.focus();
    input.type_text("8");
    harness.run();
    assert!(
        actions.borrow().iter().any(|action| matches!(
            action,
            VideoExportAction::Text { id: 10, value } if value.contains('8')
        )),
        "分辨率输入应回传 Text(10)，实际 {:?}",
        actions.borrow()
    );
}

/// 计数器 / 数据曲线分区在 egui 渲染器里的可访问性与回传。
fn rich_model() -> VideoExportModel {
    let cell = |label: &str, value: &str, id: u16, width: f32| TextPairCell {
        label: label.to_owned(),
        label_width: 110.0,
        value: value.to_owned(),
        hint: "0".to_owned(),
        width,
        id,
    };
    VideoExportModel {
        title: "视频导出".to_owned(),
        sections: vec![
            VideoSection {
                title: "计数器设置".to_owned(),
                rows: vec![
                    VideoRow::TextArea {
                        label: String::new(),
                        value: "Notes: {nc}".to_owned(),
                        hint: "Notes".to_owned(),
                        height: 140.0,
                        id: 100,
                    },
                    VideoRow::Buttons {
                        buttons: vec![
                            VideoButton {
                                label: "恢复默认".to_owned(),
                                id: 101,
                                fill: VideoButtonFill::Primary,
                                width: 0.0,
                            },
                            VideoButton {
                                label: "完整模板".to_owned(),
                                id: 102,
                                fill: VideoButtonFill::Primary,
                                width: 0.0,
                            },
                        ],
                    },
                    VideoRow::Gap { height: 12.0 },
                    VideoRow::Pick {
                        label: "对齐:".to_owned(),
                        label_width: 100.0,
                        options: vec!["左上".to_owned(), "右下".to_owned()],
                        selected: 0,
                        width: 140.0,
                        id: 107,
                    },
                    VideoRow::Slider {
                        label: "字号:".to_owned(),
                        label_width: 100.0,
                        value: 40.0,
                        min: 7.0,
                        max: 256.0,
                        step: 1.0,
                        slider_width: 140.0,
                        value_text: "40 px".to_owned(),
                        id: 108,
                    },
                    VideoRow::Toggle {
                        label: "使用千分位分隔符（1,234,567）".to_owned(),
                        checked: true,
                        id: 109,
                    },
                    VideoRow::TextPair {
                        left: cell("BPM 整数", "3", 120, 56.0),
                        right: cell("BPM 小数", "2", 121, 56.0),
                        separator: String::new(),
                    },
                    VideoRow::TextButton {
                        label: "路径:".to_owned(),
                        label_width: 50.0,
                        value: "C:/a.csv".to_owned(),
                        hint: "选择 CSV 输出路径...".to_owned(),
                        width: 0.0,
                        text_id: 112,
                        button: VideoButton {
                            label: "浏览...".to_owned(),
                            id: 113,
                            fill: VideoButtonFill::Primary,
                            width: 0.0,
                        },
                    },
                ],
            },
            VideoSection {
                title: "数据曲线设置".to_owned(),
                rows: vec![
                    VideoRow::Pick {
                        label: "数据来源:".to_owned(),
                        label_width: 100.0,
                        options: vec!["NPS（每秒音符数）".to_owned(), "复音数".to_owned()],
                        selected: 0,
                        width: 180.0,
                        id: 200,
                    },
                    VideoRow::TextPair {
                        left: cell("背景色", "#000000", 210, 96.0),
                        right: cell("折线色", "#00FFFF", 211, 96.0),
                        separator: String::new(),
                    },
                    VideoRow::Toggle {
                        label: "显示水平网格线".to_owned(),
                        checked: true,
                        id: 236,
                    },
                ],
            },
        ],
        preview: None,
        buttons: vec![VideoButton {
            label: "开始导出".to_owned(),
            id: 18,
            fill: VideoButtonFill::Primary,
            width: 120.0,
        }],
    }
}

fn make_rich_harness() -> (Harness<'static>, Rc<RefCell<Vec<VideoExportAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(760.0, 1400.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            let panel = VideoExportPanelModel {
                rect: egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(760.0, 1400.0)),
                content: rich_model(),
            };
            video_export::show_panel(ui, &panel);
            if let Some(action) = video_export::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    (harness, actions)
}

#[test]
fn counter_section_controls_are_visible() {
    let (harness, _) = make_rich_harness();
    for label in [
        "计数器设置",
        "恢复默认",
        "完整模板",
        "对齐:",
        "字号:",
        "40 px",
        "使用千分位分隔符（1,234,567）",
        "BPM 整数",
        "BPM 小数",
        "路径:",
        "浏览...",
    ] {
        assert!(
            harness.query_by_label(label).is_some(),
            "缺少可访问标签：{label}"
        );
    }
    // 多行文本域的内容通过 value 暴露（不是 label）。
    assert!(
        harness
            .query_all_by(|node| node.value().as_deref() == Some("Notes: {nc}"))
            .count()
            >= 1,
        "缺少模板文本域内容"
    );
}

#[test]
fn data_curve_section_controls_are_visible() {
    let (harness, _) = make_rich_harness();
    for label in ["数据曲线设置", "数据来源:", "折线色", "显示水平网格线"] {
        assert!(
            harness.query_by_label(label).is_some(),
            "缺少可访问标签：{label}"
        );
    }
}

#[test]
fn counter_buttons_and_pads_dispatch() {
    let (mut harness, actions) = make_rich_harness();
    harness.get_by_label("完整模板").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&VideoExportAction::Button { id: 102 })
    );

    let (mut harness, actions) = make_rich_harness();
    harness
        .get_all_by_label("浏览...")
        .next()
        .expect("应有浏览按钮")
        .click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&VideoExportAction::Button { id: 113 })
    );

    // 补零宽度的第二列（BPM 小数）输入。
    let (mut harness, actions) = make_rich_harness();
    let input = harness.get_by(|node| {
        node.role() == egui::Role::TextInput && node.value().as_deref() == Some("2")
    });
    input.focus();
    input.type_text("5");
    harness.run();
    assert!(
        actions
            .borrow()
            .iter()
            .any(|action| matches!(action, VideoExportAction::Text { id: 121, .. })),
        "补零输入应回传 Text(121)，实际 {:?}",
        actions.borrow()
    );
}

#[test]
fn template_text_area_dispatches_full_text() {
    let (mut harness, actions) = make_rich_harness();
    let area = harness.get_by(|node| node.role() == egui::Role::MultilineTextInput);
    area.focus();
    area.type_text("X");
    harness.run();
    assert!(
        actions
            .borrow()
            .iter()
            .any(|action| matches!(action, VideoExportAction::Text { id: 100, value } if value.contains('X'))),
        "模板编辑应回传 Text(100)，实际 {:?}",
        actions.borrow()
    );
}
