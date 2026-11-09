//! 视频导出面板：生产状态 → egui 模型，以及通用控件动作 → `VideoExportAction`。
//!
//! 覆盖全部渲染模式的设置：MIDI 数据源 / 渲染设置（含 MidiConsole）/
//! 计数器（`counter` 子模块）/ 数据曲线（`data_curve` 子模块）/ 预览 / 导出位置 / 按钮。

mod counter;
mod data_curve;
mod rows;

use lumino_core::tr;
use lumino_message::events::window::video::RenderMode;
use lumino_ui_egui::chrome::{
    VideoButtonFill, VideoExportModel, VideoExportPanelModel, VideoPreviewModel, VideoRow,
    VideoSection,
};
use lumino_ui_egui::video_export::VideoExportAction as EguiAction;

use rows::{button, gap, label, pick, text, text_button};

use crate::Message;
use crate::message::VideoExportAction as Prod;
use crate::state::root_state::{MIDITRAIL_Z_FAR_MAX, VideoExportDialogState};
use crate::view::video_export_dialog::counter_settings_font::SYSTEM_FONT_FAMILIES;
use crate::view::video_export_dialog::helpers;

impl crate::root::Root {
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
                103 => Prod::CounterFontModeChanged(counter::FONT_MODES.get(*index)?.to_string()),
                104 => {
                    Prod::CounterFontFamilyChanged(SYSTEM_FONT_FAMILIES.get(*index)?.to_string())
                }
                107 => Prod::CounterAlignmentChanged(counter::ALIGNMENTS.get(*index)?.to_string()),
                200 => Prod::DataCurveTextChanged {
                    field: "metric".to_string(),
                    value: data_curve::METRICS.get(*index)?.to_string(),
                },
                217 => Prod::DataCurveFontModeChanged(counter::FONT_MODES.get(*index)?.to_string()),
                218 => Prod::DataCurveTextChanged {
                    field: "font_family".to_string(),
                    value: SYSTEM_FONT_FAMILIES.get(*index)?.to_string(),
                },
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
                108 => Prod::CounterFontSizeChanged(value.round().clamp(7.0, 256.0) as u32),
                216 => Prod::DataCurveFontSizeChanged(value.round().clamp(7.0, 256.0) as u32),
                _ => return None,
            },
            EguiAction::Toggle { id, value } => match id {
                9 => Prod::Miditrail3DNotesChanged(*value),
                109 => Prod::CounterUseCommasChanged(*value),
                110 => Prod::CounterPaddingZeroesChanged(*value),
                111 => Prod::CounterSaveCsvChanged(*value),
                234 => Prod::DataCurveBoolChanged {
                    field: "abbreviate".to_string(),
                    value: *value,
                },
                235 => Prod::DataCurveBoolChanged {
                    field: "show_text".to_string(),
                    value: *value,
                },
                236 => Prod::DataCurveBoolChanged {
                    field: "show_bars".to_string(),
                    value: *value,
                },
                _ => return None,
            },
            EguiAction::Text { id, value } => match id {
                10 => Prod::WidthChanged(value.clone()),
                11 => Prod::HeightChanged(value.clone()),
                14 => Prod::OutputPathChanged(value.clone()),
                100 => Prod::CounterTextChanged(value.clone()),
                105 => Prod::CounterFontPathChanged(value.clone()),
                112 => Prod::CounterCsvPathChanged(value.clone()),
                114 => Prod::CounterCsvFormatChanged(value.clone()),
                120..=127 => Prod::CounterPadChanged {
                    field: counter::PAD_FIELDS.get(usize::from(*id) - 120)?.to_string(),
                    value: value.trim().parse::<u32>().unwrap_or(0),
                },
                201 => curve_number("graph_duration", value)?,
                202 => curve_number("zoom_smoothness", value)?,
                203 => curve_number("graph_smoothness", value)?,
                204 => curve_number("padding_mul", value)?,
                210 => curve_text("bg_color", value),
                211 => curve_text("line_color", value),
                212 => curve_text("text_color", value),
                213 => curve_text("bar_color", value),
                214 => curve_number("line_thickness", value)?,
                215 => curve_number("bar_thickness", value)?,
                219 => curve_text("font_path", value),
                230 => curve_number("text_x_offset", value)?,
                231 => curve_number("text_y_offset", value)?,
                232 => curve_number("milestone_scale_mul", value)?,
                233 => curve_number("abbreviate_digits", value)?,
                _ => return None,
            },
            EguiAction::Button { id } => match id {
                15 => Prod::BrowseMidi,
                16 => Prod::BrowseOutput,
                17 => Prod::ClosePanel,
                18 => Prod::StartExport,
                101 => Prod::CounterLoadTemplate("default".to_string()),
                102 => Prod::CounterLoadTemplate("full".to_string()),
                106 => Prod::CounterBrowseFont,
                113 => Prod::CounterBrowseCsv,
                128 => Prod::CounterResetPadding,
                220 => Prod::DataCurveBrowseFont,
                _ => return None,
            },
        };
        Some(Message::VideoExport(prod))
    }
}

/// 数据曲线数值型字段（解析失败按 0 处理，与 iced 视图同口径）。
fn curve_number(field: &str, value: &str) -> Option<Prod> {
    Some(Prod::DataCurveNumberChanged {
        field: field.to_string(),
        value: value.trim().parse::<f32>().unwrap_or(0.0),
    })
}

/// 数据曲线文本型字段（颜色 / 字体路径）。
fn curve_text(field: &str, value: &str) -> Prod {
    Prod::DataCurveTextChanged {
        field: field.to_string(),
        value: value.to_string(),
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
            200.0,
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
            200.0,
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
            200.0,
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
            200.0,
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
            200.0,
            5,
        ),
        VideoRow::Slider {
            label: tr!("下落速度:").to_string(),
            label_width: 100.0,
            value: speed,
            min: 0.1,
            max: 10.0,
            step: 0.1,
            slider_width: 200.0,
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
            200.0,
            7,
        ));
        settings.push(VideoRow::Slider {
            label: tr!("Z 显示距离:").to_string(),
            label_width: 100.0,
            value: state.miditrail_z_far,
            min: 0.1,
            max: MIDITRAIL_Z_FAR_MAX,
            step: 0.1,
            slider_width: 200.0,
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
        200.0,
        12,
    ));
    sections.push(VideoSection {
        title: tr!("渲染设置").to_string(),
        rows: settings,
    });

    // 计数器 / 数据曲线附加设置（各自独立分区，与 iced 顺序一致）。
    if state.render_mode == "计数器" {
        sections.push(counter::section(state));
    }
    if state.render_mode == "数据曲线" {
        sections.push(data_curve::section(state));
    }

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
                200.0,
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
