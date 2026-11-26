//! 视频导出面板：生产状态 → egui 模型，以及通用控件动作 → `VideoExportAction`。
//!
//! 覆盖全部渲染模式的设置：MIDI 数据源 / 渲染设置（含 MidiConsole）/
//! 计数器（`counter` 子模块）/ 数据曲线（`data_curve` 子模块）/ 导出位置 / 按钮。

mod backends;
mod counter;
mod data_curve;
mod rows;

use lumino_core::tr;
use lumino_message::events::window::video::RenderMode;
use lumino_ui_egui::chrome::{
    TextPairCell, VideoButtonFill, VideoExportModel, VideoExportPanelModel, VideoRow, VideoSection,
};
use lumino_ui_egui::video_export::VideoExportAction as EguiAction;

use rows::{button, gap, label, pick, text, text_button};

use crate::Message;
use crate::message::VideoExportAction as Prod;
use crate::state::root_state::{MIDITRAIL_Z_FAR_MAX, VideoExportDialogState};
use backends::{SYSTEM_FONT_FAMILIES, available_backends};

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
        use rows::ids;
        let state = &self.state.video_export_dialog;
        let prod = match action {
            EguiAction::Pick { id, index } => match *id {
                ids::CONTAINER => {
                    Prod::ContainerChanged(["MP4", "MOV", "MKV", "AVI"].get(*index)?.to_string())
                }
                ids::CODEC => Prod::CodecChanged(
                    ["H.264", "H.265 / HEVC", "ProRes", "VP9", "AV1"]
                        .get(*index)?
                        .to_string(),
                ),
                ids::BACKEND => Prod::BackendChanged(available_backends().get(*index)?.clone()),
                ids::QUALITY => Prod::QualityChanged(["低", "中", "高"].get(*index)?.to_string()),
                ids::RENDER_MODE => {
                    Prod::RenderModeChanged(RenderMode::ALL.get(*index)?.to_string())
                }
                ids::MIDITRAIL_VIEW_MODE => {
                    Prod::MiditrailViewModeChanged(["Normal", "Top"].get(*index)?.to_string())
                }
                ids::FPS => Prod::FpsChanged(*[24u32, 30, 60, 120].get(*index)?),
                ids::MIDI_CONSOLE_BACKEND => {
                    Prod::MidiConsoleBackendChanged(["GPU", "CPU"].get(*index)?.to_string())
                }
                ids::COUNTER_FONT_MODE => {
                    Prod::CounterFontModeChanged(counter::FONT_MODES.get(*index)?.to_string())
                }
                ids::COUNTER_FONT_FAMILY => {
                    Prod::CounterFontFamilyChanged(SYSTEM_FONT_FAMILIES.get(*index)?.to_string())
                }
                ids::COUNTER_ALIGNMENT => {
                    Prod::CounterAlignmentChanged(counter::ALIGNMENTS.get(*index)?.to_string())
                }
                ids::DC_METRIC => Prod::DataCurveTextChanged {
                    field: "metric".to_string(),
                    value: data_curve::METRICS.get(*index)?.to_string(),
                },
                ids::DC_FONT_MODE => {
                    Prod::DataCurveFontModeChanged(counter::FONT_MODES.get(*index)?.to_string())
                }
                ids::DC_FONT_FAMILY => Prod::DataCurveTextChanged {
                    field: "font_family".to_string(),
                    value: SYSTEM_FONT_FAMILIES.get(*index)?.to_string(),
                },
                _ => return None,
            },
            EguiAction::Slider { id, value } => match *id {
                ids::WATERFALL_SPEED => {
                    if state.basic.render_mode == "MIDITrail" {
                        if state.miditrail.view_mode == "Top" {
                            Prod::MiditrailTopSpeedChanged(*value)
                        } else {
                            Prod::MiditrailNormalSpeedChanged(*value)
                        }
                    } else {
                        Prod::WaterfallSpeedChanged(*value)
                    }
                }
                ids::MIDITRAIL_Z_FAR => Prod::MiditrailZFarChanged(*value),
                ids::COUNTER_FONT_SIZE => {
                    Prod::CounterFontSizeChanged(value.round().clamp(7.0, 256.0) as u32)
                }
                ids::DC_FONT_SIZE => {
                    Prod::DataCurveFontSizeChanged(value.round().clamp(7.0, 256.0) as u32)
                }
                _ => return None,
            },
            EguiAction::Toggle { id, value } => match *id {
                ids::MIDITRAIL_3D_NOTES => Prod::Miditrail3DNotesChanged(*value),
                ids::COUNTER_USE_COMMAS => Prod::CounterUseCommasChanged(*value),
                ids::COUNTER_PADDING_ZEROES => Prod::CounterPaddingZeroesChanged(*value),
                ids::COUNTER_SAVE_CSV => Prod::CounterSaveCsvChanged(*value),
                ids::DC_ABBREVIATE => Prod::DataCurveBoolChanged {
                    field: "abbreviate".to_string(),
                    value: *value,
                },
                ids::DC_SHOW_TEXT => Prod::DataCurveBoolChanged {
                    field: "show_text".to_string(),
                    value: *value,
                },
                ids::DC_SHOW_BARS => Prod::DataCurveBoolChanged {
                    field: "show_bars".to_string(),
                    value: *value,
                },
                _ => return None,
            },
            EguiAction::Text { id, value } => match *id {
                ids::WIDTH => Prod::WidthChanged(value.clone()),
                ids::HEIGHT => Prod::HeightChanged(value.clone()),
                ids::OUTPUT_PATH => Prod::OutputPathChanged(value.clone()),
                ids::COUNTER_TEXT => Prod::CounterTextChanged(value.clone()),
                ids::COUNTER_FONT_PATH => Prod::CounterFontPathChanged(value.clone()),
                ids::COUNTER_CSV_PATH => Prod::CounterCsvPathChanged(value.clone()),
                ids::COUNTER_CSV_FORMAT => Prod::CounterCsvFormatChanged(value.clone()),
                id if id.get() >= ids::COUNTER_PAD_FIRST.get()
                    && id.get() <= ids::COUNTER_PAD_LAST.get() =>
                {
                    Prod::CounterPadChanged {
                        field: counter::PAD_FIELDS
                            .get(usize::from(id.get()) - 120)?
                            .to_string(),
                        value: value.trim().parse::<u32>().unwrap_or(0),
                    }
                }
                ids::DC_GRAPH_DURATION => curve_number("graph_duration", value)?,
                ids::DC_ZOOM_SMOOTHNESS => curve_number("zoom_smoothness", value)?,
                ids::DC_GRAPH_SMOOTHNESS => curve_number("graph_smoothness", value)?,
                ids::DC_PADDING_MUL => curve_number("padding_mul", value)?,
                ids::DC_BG_COLOR => curve_text("bg_color", value),
                ids::DC_LINE_COLOR => curve_text("line_color", value),
                ids::DC_TEXT_COLOR => curve_text("text_color", value),
                ids::DC_BAR_COLOR => curve_text("bar_color", value),
                ids::DC_LINE_THICKNESS => curve_number("line_thickness", value)?,
                ids::DC_BAR_THICKNESS => curve_number("bar_thickness", value)?,
                ids::DC_FONT_PATH => curve_text("font_path", value),
                ids::DC_TEXT_X_OFFSET => curve_number("text_x_offset", value)?,
                ids::DC_TEXT_Y_OFFSET => curve_number("text_y_offset", value)?,
                ids::DC_MILESTONE_SCALE => curve_number("milestone_scale_mul", value)?,
                ids::DC_ABBREVIATE_DIGITS => curve_number("abbreviate_digits", value)?,
                _ => return None,
            },
            EguiAction::Button { id } => match *id {
                ids::BROWSE_MIDI => Prod::BrowseMidi,
                ids::BROWSE_OUTPUT => Prod::BrowseOutput,
                ids::CLOSE_PANEL => Prod::ClosePanel,
                ids::START_EXPORT => Prod::StartExport,
                ids::COUNTER_TEMPLATE_DEFAULT => Prod::CounterLoadTemplate("default".to_string()),
                ids::COUNTER_TEMPLATE_FULL => Prod::CounterLoadTemplate("full".to_string()),
                ids::COUNTER_BROWSE_FONT => Prod::CounterBrowseFont,
                ids::COUNTER_BROWSE_CSV => Prod::CounterBrowseCsv,
                ids::COUNTER_RESET_PADDING => Prod::CounterResetPadding,
                ids::DC_BROWSE_FONT => Prod::DataCurveBrowseFont,
                _ => return None,
            },
        };
        Some(Message::VideoExport(prod))
    }
}

/// 数据曲线数值型字段（解析失败按 0 处理，与 上一代 视图同口径）。
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
            text("", 0.0, &state.basic.midi_path, "", 0.0, 14),
            label(
                if state.basic.midi_path.is_empty() {
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
    let is_miditrail = state.basic.render_mode == "MIDITrail";
    let is_top = state.miditrail.view_mode == "Top";
    let speed = if is_miditrail {
        if is_top {
            state.miditrail.top_speed
        } else {
            state.miditrail.normal_speed
        }
    } else {
        state.basic.waterfall_speed
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
                .position(|v| *v == state.basic.container)
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
                .position(|v| *v == state.basic.codec)
                .unwrap_or(0),
            200.0,
            2,
        ),
        pick(
            tr!("加速:"),
            100.0,
            available_backends(),
            available_backends()
                .iter()
                .position(|v| *v == state.basic.backend)
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
                .position(|v| *v == state.basic.quality)
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
                .position(|m| m.to_string() == state.basic.render_mode)
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
            id: 6.into(),
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
            value: state.miditrail.z_far,
            min: 0.1,
            max: MIDITRAIL_Z_FAR_MAX,
            step: 0.1,
            slider_width: 200.0,
            value_text: format!("{:.1}", state.miditrail.z_far),
            id: 8.into(),
        });
        settings.push(VideoRow::Toggle {
            label: tr!("3D 音符（盒子，关闭则为平面）").to_string(),
            checked: state.miditrail.notes_3d,
            id: 9.into(),
        });
    }
    settings.push(VideoRow::Buttons { buttons: vec![] });
    settings.push(VideoRow::TextPair {
        left: TextPairCell {
            label: tr!("分辨率:").to_string(),
            label_width: 100.0,
            value: state.basic.width.to_string(),
            hint: "1920".to_string(),
            width: 80.0,
            id: 10.into(),
        },
        right: TextPairCell {
            label: String::new(),
            label_width: 0.0,
            value: state.basic.height.to_string(),
            hint: "1080".to_string(),
            width: 80.0,
            id: 11.into(),
        },
        separator: "x".to_string(),
    });
    settings.push(pick(
        tr!("帧率:"),
        100.0,
        [24u32, 30, 60, 120].iter().map(|v| v.to_string()).collect(),
        [24u32, 30, 60, 120]
            .iter()
            .position(|v| *v == state.basic.fps)
            .unwrap_or(1),
        200.0,
        12,
    ));
    sections.push(VideoSection {
        title: tr!("渲染设置").to_string(),
        rows: settings,
    });

    // 计数器 / 数据曲线附加设置（各自独立分区，与 上一代 顺序一致）。
    if state.basic.render_mode == "计数器" {
        sections.push(counter::section(state));
    }
    if state.basic.render_mode == "数据曲线" {
        sections.push(data_curve::section(state));
    }

    // MidiConsole 附加设置。
    if state.basic.render_mode == "MidiConsole" {
        sections.push(VideoSection {
            title: tr!("MidiConsole 设置").to_string(),
            rows: vec![pick(
                tr!("渲染后端:"),
                100.0,
                vec!["GPU".to_string(), "CPU".to_string()],
                if state.basic.midi_console_backend == "CPU" {
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
            text(
                "",
                0.0,
                &state.basic.output_path,
                tr!("选择输出路径..."),
                0.0,
                14,
            ),
            VideoRow::Buttons {
                buttons: vec![button(tr!("浏览..."), 16, VideoButtonFill::Primary, 0.0)],
            },
        ],
    });

    VideoExportModel {
        title: tr!("视频导出").to_string(),
        sections,
        buttons: vec![
            button(tr!("关闭"), 17, VideoButtonFill::Neutral, 100.0),
            button(tr!("开始导出"), 18, VideoButtonFill::Primary, 120.0),
        ],
    }
}
