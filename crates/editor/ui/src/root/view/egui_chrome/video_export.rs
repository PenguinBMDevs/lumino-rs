//! 视频导出面板：生产状态 → egui 模型，以及通用控件动作 → `VideoExportAction`。
//!
//! 本轮迁移「MIDI 数据源 / 渲染设置（含 MidiConsole）/ 预览 / 导出位置 / 按钮」；
//! 计数器与数据曲线两套附加设置控件数量大，仍由 iced 视图渲染（见计划文档）。

use lumino_core::tr;
use lumino_message::events::window::video::RenderMode;
use lumino_ui_egui::chrome::{
    VideoButton, VideoButtonFill, VideoExportModel, VideoExportPanelModel, VideoPreviewModel,
    VideoRow, VideoSection,
};
use lumino_ui_egui::video_export::VideoExportAction as EguiAction;

use crate::Message;
use crate::message::VideoExportAction as Prod;
use crate::state::root_state::{MIDITRAIL_Z_FAR_MAX, VideoExportDialogState};
use crate::view::video_export_dialog::helpers;

/// 计数器 / 数据曲线模式暂未迁移（控件多，下一步补齐）。
pub(crate) fn mode_supported(mode: &str) -> bool {
    mode != "计数器" && mode != "数据曲线"
}

fn pick(label: &str, label_width: f32, options: Vec<String>, selected: usize, id: u16) -> VideoRow {
    VideoRow::Pick {
        label: label.to_string(),
        label_width,
        options,
        selected,
        width: 200.0,
        id,
    }
}

fn text(label: &str, label_width: f32, value: &str, hint: &str, width: f32, id: u16) -> VideoRow {
    VideoRow::Text {
        label: label.to_string(),
        label_width,
        value: value.to_string(),
        hint: hint.to_string(),
        width,
        id,
    }
}

fn label(text: &str, size: f32, muted: bool) -> VideoRow {
    VideoRow::Label {
        text: text.to_string(),
        size,
        muted,
    }
}

fn button(label: &str, id: u16, fill: VideoButtonFill, width: f32) -> VideoButton {
    VideoButton {
        label: label.to_string(),
        id,
        fill,
        width,
    }
}

impl crate::root::Root {
    /// 当前渲染风格是否已由 egui 接管（计数器 / 数据曲线仍走 iced）。
    pub(crate) fn video_export_mode_supported(&self) -> bool {
        mode_supported(&self.state.video_export_dialog.render_mode)
    }

    /// 构建视频导出面板模型（`rect` = 整块右侧内容区）。
    pub(crate) fn video_export_panel_model(&self, rect: egui::Rect) -> VideoExportPanelModel {
        let state = &self.state.video_export_dialog;
        VideoExportPanelModel {
            rect,
            content: video_export_model(state),
        }
    }

    /// egui 通用控件动作 → 生产 `VideoExportAction`。
    pub(crate) fn video_export_action_message(&self, action: &EguiAction) -> Option<Message> {
        let state = &self.state.video_export_dialog;
        let prod = match action {
            EguiAction::Pick { id, index } => match id {
                1 => Prod::ContainerChanged(["MP4", "MOV", "MKV", "AVI"].get(*index)?.to_string()),
                2 => Prod::CodecChanged(
                    ["H.264", "H.265 / HEVC", "ProRes", "VP9", "AV1"]
                        .get(*index)?
                        .to_string(),
                ),
                3 => Prod::BackendChanged(helpers::available_backends().get(*index)?.clone()),
                4 => Prod::QualityChanged(["低", "中", "高"].get(*index)?.to_string()),
                5 => Prod::RenderModeChanged(RenderMode::ALL.get(*index)?.to_string()),
                7 => Prod::MiditrailViewModeChanged(["Normal", "Top"].get(*index)?.to_string()),
                12 => Prod::FpsChanged(*[24u32, 30, 60, 120].get(*index)?),
                13 => Prod::MidiConsoleBackendChanged(["GPU", "CPU"].get(*index)?.to_string()),
                _ => return None,
            },
            EguiAction::Slider { id, value } => match id {
                6 => {
                    if state.render_mode == "MIDITrail" {
                        if state.miditrail_view_mode == "Top" {
                            Prod::MiditrailTopSpeedChanged(*value)
                        } else {
                            Prod::MiditrailNormalSpeedChanged(*value)
                        }
                    } else {
                        Prod::WaterfallSpeedChanged(*value)
                    }
                }
                8 => Prod::MiditrailZFarChanged(*value),
                _ => return None,
            },
            EguiAction::Toggle { id, value } => match id {
                9 => Prod::Miditrail3DNotesChanged(*value),
                _ => return None,
            },
            EguiAction::Text { id, value } => match id {
                10 => Prod::WidthChanged(value.clone()),
                11 => Prod::HeightChanged(value.clone()),
                14 => Prod::OutputPathChanged(value.clone()),
                _ => return None,
            },
            EguiAction::Button { id } => match id {
                15 => Prod::BrowseMidi,
                16 => Prod::BrowseOutput,
                17 => Prod::ClosePanel,
                18 => Prod::StartExport,
                _ => return None,
            },
        };
        Some(Message::VideoExport(prod))
    }
}

/// 生成面板内容。
fn video_export_model(state: &VideoExportDialogState) -> VideoExportModel {
    let mut sections = Vec::new();

    // MIDI 数据源。
    sections.push(VideoSection {
        title: tr!("MIDI 数据源").to_string(),
        rows: vec![
            text("", 0.0, &state.midi_path, "", 0.0, 14),
            label(
                if state.midi_path.is_empty() {
                    tr!("优先使用当前工程的 MIDI 数据")
                } else {
                    tr!("使用指定 MIDI 文件流式读取")
                },
                12.0,
                true,
            ),
            VideoRow::Buttons {
                buttons: vec![button(tr!("浏览..."), 15, VideoButtonFill::Primary, 0.0)],
            },
        ],
    });

    // 渲染设置。
    let is_miditrail = state.render_mode == "MIDITrail";
    let is_top = state.miditrail_view_mode == "Top";
    let speed = if is_miditrail {
        if is_top {
            state.miditrail_top_speed
        } else {
            state.miditrail_normal_speed
        }
    } else {
        state.waterfall_speed
    };
    let mut settings = vec![
        pick(
            tr!("渲染格式:"),
            100.0,
            ["MP4", "MOV", "MKV", "AVI"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            ["MP4", "MOV", "MKV", "AVI"]
                .iter()
                .position(|v| *v == state.container)
                .unwrap_or(0),
            1,
        ),
        pick(
            tr!("编码器:"),
            100.0,
            ["H.264", "H.265 / HEVC", "ProRes", "VP9", "AV1"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            ["H.264", "H.265 / HEVC", "ProRes", "VP9", "AV1"]
                .iter()
                .position(|v| *v == state.codec)
                .unwrap_or(0),
            2,
        ),
        pick(
            tr!("加速:"),
            100.0,
            helpers::available_backends(),
            helpers::available_backends()
                .iter()
                .position(|v| *v == state.backend)
                .unwrap_or(0),
            3,
        ),
        pick(
            tr!("质量:"),
            100.0,
            ["低", "中", "高"].iter().map(|s| s.to_string()).collect(),
            ["低", "中", "高"]
                .iter()
                .position(|v| *v == state.quality)
                .unwrap_or(1),
            4,
        ),
        pick(
            tr!("渲染风格:"),
            100.0,
            RenderMode::ALL.iter().map(|m| m.to_string()).collect(),
            RenderMode::ALL
                .iter()
                .position(|m| m.to_string() == state.render_mode)
                .unwrap_or(0),
            5,
        ),
        VideoRow::Slider {
            label: tr!("下落速度:").to_string(),
            label_width: 100.0,
            value: speed,
            min: 0.1,
            max: 10.0,
            step: 0.1,
            value_text: format!("{speed:.1}x"),
            id: 6,
        },
    ];
    if is_miditrail {
        settings.push(pick(
            tr!("视图模式:"),
            100.0,
            vec!["Normal".to_string(), "Top".to_string()],
            if is_top { 1 } else { 0 },
            7,
        ));
        settings.push(VideoRow::Slider {
            label: tr!("Z 显示距离:").to_string(),
            label_width: 100.0,
            value: state.miditrail_z_far,
            min: 0.1,
            max: MIDITRAIL_Z_FAR_MAX,
            step: 0.1,
            value_text: format!("{:.1}", state.miditrail_z_far),
            id: 8,
        });
        settings.push(VideoRow::Toggle {
            label: tr!("3D 音符（盒子，关闭则为平面）").to_string(),
            checked: state.miditrail_3d_notes,
            id: 9,
        });
    }
    settings.push(VideoRow::Buttons { buttons: vec![] });
    settings.push(text(
        tr!("分辨率:"),
        100.0,
        &state.width.to_string(),
        "1920",
        80.0,
        10,
    ));
    settings.push(text("", 0.0, &state.height.to_string(), "1080", 80.0, 11));
    settings.push(pick(
        tr!("帧率:"),
        100.0,
        [24u32, 30, 60, 120].iter().map(|v| v.to_string()).collect(),
        [24u32, 30, 60, 120]
            .iter()
            .position(|v| *v == state.fps)
            .unwrap_or(1),
        12,
    ));
    sections.push(VideoSection {
        title: tr!("渲染设置").to_string(),
        rows: settings,
    });

    // MidiConsole 附加设置。
    if state.render_mode == "MidiConsole" {
        sections.push(VideoSection {
            title: tr!("MidiConsole 设置").to_string(),
            rows: vec![pick(
                tr!("渲染后端:"),
                100.0,
                vec!["GPU".to_string(), "CPU".to_string()],
                if state.midi_console_backend == "CPU" {
                    1
                } else {
                    0
                },
                13,
            )],
        });
    }

    // 导出位置。
    sections.push(VideoSection {
        title: tr!("导出位置").to_string(),
        rows: vec![
            text("", 0.0, &state.output_path, tr!("选择输出路径..."), 0.0, 14),
            VideoRow::Buttons {
                buttons: vec![button(tr!("浏览..."), 16, VideoButtonFill::Primary, 0.0)],
            },
        ],
    });

    VideoExportModel {
        title: tr!("视频导出").to_string(),
        sections,
        preview: Some(VideoPreviewModel {
            label: tr!("等待渲染...").to_string(),
            image_size: state
                .cached_image_handle
                .as_ref()
                .map(|_| (state.preview_width as f32, state.preview_height as f32)),
        }),
        buttons: vec![
            button(tr!("关闭"), 17, VideoButtonFill::Neutral, 100.0),
            button(tr!("开始导出"), 18, VideoButtonFill::Primary, 120.0),
        ],
    }
}
