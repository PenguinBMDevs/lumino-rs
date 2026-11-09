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
fn mode_support_matches_gate() {
    let mut root = root();
    root.state.video_export_dialog.render_mode = "Lumino瀑布流".to_owned();
    assert!(root.video_export_mode_supported());
    root.state.video_export_dialog.render_mode = "计数器".to_owned();
    assert!(!root.video_export_mode_supported(), "计数器仍走 iced");
    root.state.video_export_dialog.render_mode = "数据曲线".to_owned();
    assert!(!root.video_export_mode_supported(), "数据曲线仍走 iced");
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
