//! 视频导出面板：模型镜像与通用控件动作映射。

use super::*;
use crate::Message;
use crate::message::VideoExportAction as Prod;
use lumino_ui_egui::chrome::{VideoButtonFill, VideoRow};
use lumino_ui_egui::video_export::VideoExportAction as EguiAction;

fn root() -> Root {
    Root::new_dialog(
        &lumino_ui_core::Theme::TokyoNightStorm.to_string(),
        crate::state::root_state::DialogType::VideoExport,
    )
}

fn rect() -> egui::Rect {
    egui::Rect::from_min_size(egui::pos2(48.0, 30.0), egui::vec2(900.0, 800.0))
}

#[test]
fn panel_model_has_all_main_sections() {
    let mut root = root();
    root.state.video_export_dialog.midi_path = "C:/a.mid".to_owned();
    root.state.video_export_dialog.output_path = "C:/out.mp4".to_owned();
    root.state.video_export_dialog.render_mode = "Lumino瀑布流".to_owned();
    let panel = root.video_export_panel_model(rect());
    assert_eq!(panel.rect, rect());
    let content = &panel.content;
    assert_eq!(content.title, "视频导出");
    let titles: Vec<&str> = content.sections.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(
        titles,
        vec!["MIDI 数据源", "渲染设置", "导出位置"],
        "瀑布流模式不应出现 MidiConsole 分区"
    );
    // 渲染设置包含 格式 / 编码器 / 加速 / 质量 / 风格 / 速度 / 分辨率 / 帧率。
    let rows = &content.sections[1].rows;
    assert!(
        rows.iter()
            .any(|r| matches!(r, VideoRow::Pick { label, id: 1, .. } if label.contains("格式")))
    );
    assert!(
        rows.iter()
            .any(|r| matches!(r, VideoRow::Slider { id: 6, .. }))
    );
    assert!(
        rows.iter()
            .any(|r| matches!(r, VideoRow::Text { id: 10, .. }))
    );
    assert!(
        rows.iter()
            .any(|r| matches!(r, VideoRow::Pick { id: 12, .. }))
    );
    // 按钮：关闭 + 开始导出。
    assert_eq!(content.buttons.len(), 2);
    assert_eq!(content.buttons[0].fill, VideoButtonFill::Neutral);
    assert_eq!(content.buttons[1].fill, VideoButtonFill::Primary);
    assert!(content.preview.is_some());
}

#[test]
fn miditrail_adds_extra_rows_and_midiconsole_section() {
    let mut root = root();
    root.state.video_export_dialog.render_mode = "MIDITrail".to_owned();
    root.state.video_export_dialog.miditrail_view_mode = "Top".to_owned();
    let panel = root.video_export_panel_model(rect());
    let rows = &panel.content.sections[1].rows;
    assert!(
        rows.iter()
            .any(|r| matches!(r, VideoRow::Pick { id: 7, .. })),
        "视图模式行"
    );
    assert!(
        rows.iter()
            .any(|r| matches!(r, VideoRow::Slider { id: 8, .. })),
        "Z 距离行"
    );
    assert!(
        rows.iter()
            .any(|r| matches!(r, VideoRow::Toggle { id: 9, .. })),
        "3D 音符行"
    );

    root.state.video_export_dialog.render_mode = "MidiConsole".to_owned();
    let panel = root.video_export_panel_model(rect());
    assert!(
        panel
            .content
            .sections
            .iter()
            .any(|s| s.title.contains("MidiConsole")),
        "MidiConsole 应有独立分区"
    );
}

#[test]
fn counter_and_data_curve_sections_are_built() {
    let mut root = root();
    root.state.video_export_dialog.render_mode = "计数器".to_owned();
    let panel = root.video_export_panel_model(rect());
    assert!(
        panel
            .content
            .sections
            .iter()
            .any(|s| s.title == "计数器设置"),
        "计数器模式应有独立分区"
    );
    let counter = panel
        .content
        .sections
        .iter()
        .find(|s| s.title == "计数器设置")
        .expect("计数器分区");
    assert!(
        counter
            .rows
            .iter()
            .any(|r| matches!(r, VideoRow::TextArea { id: 100, .. })),
        "模板编辑区"
    );
    assert!(
        counter
            .rows
            .iter()
            .any(|r| matches!(r, VideoRow::Slider { id: 108, .. })),
        "字号滑杆"
    );
    // 8 个补零输入（两两一行，共 4 行 TextPair）。
    let pad_rows = counter
        .rows
        .iter()
        .filter(|r| matches!(r, VideoRow::TextPair { .. }))
        .count();
    assert_eq!(pad_rows, 4, "补零宽度应有 4 行两列");

    root.state.video_export_dialog.render_mode = "数据曲线".to_owned();
    let panel = root.video_export_panel_model(rect());
    assert!(
        panel
            .content
            .sections
            .iter()
            .any(|s| s.title == "数据曲线设置"),
        "数据曲线模式应有独立分区"
    );
    let curve = panel
        .content
        .sections
        .iter()
        .find(|s| s.title == "数据曲线设置")
        .expect("数据曲线分区");
    assert!(
        curve
            .rows
            .iter()
            .any(|r| matches!(r, VideoRow::Pick { id: 200, .. })),
        "数据来源下拉"
    );
    assert!(
        curve
            .rows
            .iter()
            .any(|r| matches!(r, VideoRow::Toggle { id: 234, .. })),
        "缩写开关"
    );
}

#[test]
fn widget_actions_map_to_messages() {
    let root = root();
    assert!(matches!(
        root.video_export_action_message(&EguiAction::Pick { id: 1, index: 2 }),
        Some(Message::VideoExport(Prod::ContainerChanged(v))) if v == "MKV"
    ));
    assert!(matches!(
        root.video_export_action_message(&EguiAction::Pick { id: 12, index: 2 }),
        Some(Message::VideoExport(Prod::FpsChanged(60)))
    ));
    assert!(matches!(
        root.video_export_action_message(&EguiAction::Toggle { id: 9, value: true }),
        Some(Message::VideoExport(Prod::Miditrail3DNotesChanged(true)))
    ));
    assert!(matches!(
        root.video_export_action_message(&EguiAction::Text {
            id: 11,
            value: "720".to_owned()
        }),
        Some(Message::VideoExport(Prod::HeightChanged(v))) if v == "720"
    ));
    assert!(matches!(
        root.video_export_action_message(&EguiAction::Button { id: 18 }),
        Some(Message::VideoExport(Prod::StartExport))
    ));
    // 越界下标 / 未知 id 不产生消息。
    assert!(
        root.video_export_action_message(&EguiAction::Pick { id: 1, index: 9 })
            .is_none()
    );
    assert!(
        root.video_export_action_message(&EguiAction::Button { id: 99 })
            .is_none()
    );
}

#[test]
fn counter_section_rows_cover_every_control() {
    let mut root = root();
    root.state.video_export_dialog.render_mode = "计数器".to_owned();
    root.state.video_export_dialog.counter_save_csv = true;
    let panel = root.video_export_panel_model(rect());
    let counter = panel
        .content
        .sections
        .iter()
        .find(|s| s.title == "计数器设置")
        .expect("计数器分区");
    let ids: Vec<&str> = counter
        .rows
        .iter()
        .map(|row| match row {
            VideoRow::TextArea { .. } => "textarea",
            VideoRow::Pick { .. } => "pick",
            VideoRow::Slider { .. } => "slider",
            VideoRow::Toggle { .. } => "toggle",
            VideoRow::Text { .. } => "text",
            VideoRow::TextButton { .. } => "textbutton",
            VideoRow::TextPair { .. } => "pair",
            VideoRow::Buttons { .. } => "buttons",
            VideoRow::Gap { .. } => "gap",
            VideoRow::Label { .. } => "label",
        })
        .collect();
    for expected in [
        "textarea",
        "pick",
        "slider",
        "toggle",
        "text",
        "textbutton",
        "pair",
    ] {
        assert!(
            ids.contains(&expected),
            "计数器分区缺少 {expected} 行：{ids:?}"
        );
    }
    // save_csv 打开时才有 CSV 路径 / 格式行。
    assert!(
        counter
            .rows
            .iter()
            .any(|r| matches!(r, VideoRow::TextButton { text_id: 112, .. })),
        "CSV 路径行"
    );
    assert!(
        counter
            .rows
            .iter()
            .any(|r| matches!(r, VideoRow::Text { id: 114, .. })),
        "CSV 格式行"
    );

    root.state.video_export_dialog.counter_save_csv = false;
    let panel = root.video_export_panel_model(rect());
    let counter = panel
        .content
        .sections
        .iter()
        .find(|s| s.title == "计数器设置")
        .expect("计数器分区");
    assert!(
        !counter
            .rows
            .iter()
            .any(|r| matches!(r, VideoRow::TextButton { text_id: 112, .. })),
        "关闭 CSV 后不应有路径行"
    );
}

#[test]
fn counter_font_mode_branches() {
    let mut root = root();
    root.state.video_export_dialog.render_mode = "计数器".to_owned();

    root.state.video_export_dialog.counter_font_mode = "系统字体".to_owned();
    let panel = root.video_export_panel_model(rect());
    let counter = panel
        .content
        .sections
        .iter()
        .find(|s| s.title == "计数器设置")
        .expect("计数器分区");
    assert!(
        counter
            .rows
            .iter()
            .any(|r| matches!(r, VideoRow::Pick { id: 104, .. })),
        "系统字体分支应有字体族下拉"
    );

    root.state.video_export_dialog.counter_font_mode = "自定义字体".to_owned();
    let panel = root.video_export_panel_model(rect());
    let counter = panel
        .content
        .sections
        .iter()
        .find(|s| s.title == "计数器设置")
        .expect("计数器分区");
    assert!(
        counter
            .rows
            .iter()
            .any(|r| matches!(r, VideoRow::TextButton { text_id: 105, .. })),
        "自定义字体分支应有路径输入"
    );
}

#[test]
fn data_curve_section_rows_and_font_branches() {
    let mut root = root();
    root.state.video_export_dialog.render_mode = "数据曲线".to_owned();
    root.state.video_export_dialog.dc_font_mode = "系统字体".to_owned();
    let panel = root.video_export_panel_model(rect());
    let curve = panel
        .content
        .sections
        .iter()
        .find(|s| s.title == "数据曲线设置")
        .expect("数据曲线分区");
    // 4 个两列数值/颜色行 + 3 个开关 + 数据来源下拉 + 字号滑杆 + 字体下拉。
    assert!(
        curve
            .rows
            .iter()
            .any(|r| matches!(r, VideoRow::Pick { id: 200, .. }))
    );
    assert!(
        curve
            .rows
            .iter()
            .any(|r| matches!(r, VideoRow::Pick { id: 217, .. }))
    );
    assert!(
        curve
            .rows
            .iter()
            .any(|r| matches!(r, VideoRow::Pick { id: 218, .. }))
    );
    assert!(
        curve
            .rows
            .iter()
            .any(|r| matches!(r, VideoRow::Slider { id: 216, .. }))
    );
    assert!(
        curve
            .rows
            .iter()
            .any(|r| matches!(r, VideoRow::Toggle { id: 234, .. }))
    );
    assert!(
        curve
            .rows
            .iter()
            .any(|r| matches!(r, VideoRow::Toggle { id: 235, .. }))
    );
    assert!(
        curve
            .rows
            .iter()
            .any(|r| matches!(r, VideoRow::Toggle { id: 236, .. }))
    );
    let pairs = curve
        .rows
        .iter()
        .filter(|r| matches!(r, VideoRow::TextPair { .. }))
        .count();
    assert_eq!(pairs, 7, "曲线参数 2 + 外观 3 + 文字 2 = 7 行两列");

    root.state.video_export_dialog.dc_font_mode = "自定义字体".to_owned();
    let panel = root.video_export_panel_model(rect());
    let curve = panel
        .content
        .sections
        .iter()
        .find(|s| s.title == "数据曲线设置")
        .expect("数据曲线分区");
    assert!(
        curve
            .rows
            .iter()
            .any(|r| matches!(r, VideoRow::TextButton { text_id: 219, .. }))
    );
}

#[test]
fn counter_and_data_curve_actions_map_to_messages() {
    let mut root = root();
    root.state.video_export_dialog.render_mode = "计数器".to_owned();

    assert!(matches!(
        root.video_export_action_message(&EguiAction::Pick { id: 103, index: 1 }),
        Some(Message::VideoExport(Prod::CounterFontModeChanged(v))) if v == "系统字体"
    ));
    assert!(matches!(
        root.video_export_action_message(&EguiAction::Pick { id: 104, index: 0 }),
        Some(Message::VideoExport(Prod::CounterFontFamilyChanged(v))) if v == "微软雅黑"
    ));
    assert!(matches!(
        root.video_export_action_message(&EguiAction::Pick { id: 107, index: 0 }),
        Some(Message::VideoExport(Prod::CounterAlignmentChanged(v))) if v == "左上"
    ));
    assert!(matches!(
        root.video_export_action_message(&EguiAction::Slider {
            id: 108,
            value: 42.4
        }),
        Some(Message::VideoExport(Prod::CounterFontSizeChanged(42)))
    ));
    assert!(matches!(
        root.video_export_action_message(&EguiAction::Toggle {
            id: 109,
            value: false
        }),
        Some(Message::VideoExport(Prod::CounterUseCommasChanged(false)))
    ));
    assert!(matches!(
        root.video_export_action_message(&EguiAction::Text {
            id: 100,
            value: "hi".to_owned()
        }),
        Some(Message::VideoExport(Prod::CounterTextChanged(v))) if v == "hi"
    ));
    assert!(matches!(
        root.video_export_action_message(&EguiAction::Text {
            id: 121,
            value: "7".to_owned()
        }),
        Some(Message::VideoExport(Prod::CounterPadChanged { field, value }))
            if field == "bpm_dec" && value == 7
    ));
    assert!(matches!(
        root.video_export_action_message(&EguiAction::Button { id: 102 }),
        Some(Message::VideoExport(Prod::CounterLoadTemplate(v))) if v == "full"
    ));
    assert!(matches!(
        root.video_export_action_message(&EguiAction::Button { id: 128 }),
        Some(Message::VideoExport(Prod::CounterResetPadding))
    ));
    assert!(matches!(
        root.video_export_action_message(&EguiAction::Button { id: 106 }),
        Some(Message::VideoExport(Prod::CounterBrowseFont))
    ));

    // 数据曲线。
    assert!(matches!(
        root.video_export_action_message(&EguiAction::Pick { id: 200, index: 1 }),
        Some(Message::VideoExport(Prod::DataCurveTextChanged { field, value }))
            if field == "metric" && value == "复音数"
    ));
    assert!(matches!(
        root.video_export_action_message(&EguiAction::Text {
            id: 201,
            value: "3.5".to_owned()
        }),
        Some(Message::VideoExport(Prod::DataCurveNumberChanged { field, value }))
            if field == "graph_duration" && (value - 3.5).abs() < f32::EPSILON
    ));
    assert!(matches!(
        root.video_export_action_message(&EguiAction::Text {
            id: 210,
            value: "#112233".to_owned()
        }),
        Some(Message::VideoExport(Prod::DataCurveTextChanged { field, value }))
            if field == "bg_color" && value == "#112233"
    ));
    assert!(matches!(
        root.video_export_action_message(&EguiAction::Slider {
            id: 216,
            value: 30.0
        }),
        Some(Message::VideoExport(Prod::DataCurveFontSizeChanged(30)))
    ));
    assert!(matches!(
        root.video_export_action_message(&EguiAction::Toggle { id: 236, value: false }),
        Some(Message::VideoExport(Prod::DataCurveBoolChanged { field, value }))
            if field == "show_bars" && !value
    ));
    assert!(matches!(
        root.video_export_action_message(&EguiAction::Button { id: 220 }),
        Some(Message::VideoExport(Prod::DataCurveBrowseFont))
    ));

    // 越界 / 未知 id 一律拒绝。
    assert!(
        root.video_export_action_message(&EguiAction::Pick { id: 103, index: 9 })
            .is_none()
    );
    assert!(
        root.video_export_action_message(&EguiAction::Text {
            id: 199,
            value: "x".to_owned()
        })
        .is_none()
    );
}
