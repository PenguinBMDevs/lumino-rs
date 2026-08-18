//! 视频导出面板处理器

use crate::host::DialogResult;
use crate::message::{Message, VideoExportAction};
use crate::root::Root;
use crate::state::root_state::VideoExportOverlayState;
use crate::util::parse_uint;
use std::str::FromStr;

use super::DialogHandler;

/// 重置视频导出对话框的 overlay / 预览帧 / 缓存，并在对话框窗口中登记取消结果。
fn reset_video_export_overlay(root: &mut Root) {
    root.state.video_export_dialog.overlay = VideoExportOverlayState::None;
    root.state.video_export_dialog.preview_frame = None;
    root.state.video_export_dialog.cached_image_handle = None;
    if root.state.is_dialog_window {
        root.state.dialog_result = Some(DialogResult::Cancel);
    }
}

/// 解析 hex 颜色字符串为 RGBA（移植自 MIDIGraphRenderer 的 hexToRGB）。
///
/// 支持 `#RRGGBB`（alpha=255）与 `#RRGGBBAA` 两种格式；解析失败返回 `None`。
fn hex_to_rgba(hex: &str) -> Option<[u8; 4]> {
    let hex = hex.trim().trim_start_matches('#');
    let bytes = match hex.len() {
        6 => u32::from_str_radix(hex, 16).ok()?,
        8 => u32::from_str_radix(hex, 16).ok()?,
        _ => return None,
    };
    match hex.len() {
        6 => Some([
            ((bytes >> 16) & 0xFF) as u8,
            ((bytes >> 8) & 0xFF) as u8,
            (bytes & 0xFF) as u8,
            255,
        ]),
        _ => Some([
            ((bytes >> 24) & 0xFF) as u8,
            ((bytes >> 16) & 0xFF) as u8,
            ((bytes >> 8) & 0xFF) as u8,
            (bytes & 0xFF) as u8,
        ]),
    }
}

/// 将数值格式化为设置面板回显字符串（整数类字段不带小数位）。
fn fmt_num(value: f32, decimals: usize) -> String {
    if decimals == 0 {
        format!("{}", value as i64)
    } else {
        format!("{value:.decimals$}")
    }
}

/// 解析带日志回退的枚举（失败不再静默：记录 warn 便于诊断非法输入）。
fn parse_enum_or_log<T>(raw: &str, name: &str) -> T
where
    T: FromStr + Default,
{
    match raw.parse::<T>() {
        Ok(v) => v,
        Err(_) => {
            tracing::warn!("视频导出配置 {name} 非法值 '{raw}'，回退默认");
            T::default()
        }
    }
}

/// 解析带日志回退的 f32（默认值随调用点语义）。
fn parse_f32_or_log(raw: &str, name: &str, default: f32) -> f32 {
    match raw.parse::<f32>() {
        Ok(v) => v,
        Err(_) => {
            tracing::warn!("视频导出配置 {name} 非法值 '{raw}'，回退 {default}");
            default
        }
    }
}

/// 从对话框状态组装视频导出配置（纯函数，可单测）。
///
/// `ppq`/`key_count` 来自编辑器视图；非法输入回退默认值并记录 warn（见 `parse_enum_or_log`）。
fn build_video_config(
    st: &crate::view::video_export_dialog::state::VideoExportDialogState,
    ppq: u16,
    key_count: u16,
) -> lumino_message::events::window::dialog::VideoExportConfig {
    use lumino_message::events::window::video::*;
    let counter_font = |mode: &str, family: String, path: String| match mode {
        "系统字体" => CounterFont::System { family },
        "自定义字体" => CounterFont::File { path },
        _ => CounterFont::Bitmap,
    };
    VideoExportConfig {
        output_path: st.output_path.clone(),
        midi_path: st.midi_path.clone(),
        width: st.width,
        height: st.height,
        fps: st.fps,
        ppq,
        key_count,
        container: parse_enum_or_log(&st.container, "container"),
        codec: parse_enum_or_log(&st.codec, "codec"),
        backend: parse_enum_or_log(&st.backend, "backend"),
        quality: parse_enum_or_log(&st.quality, "quality"),
        render_mode: parse_enum_or_log(&st.render_mode, "render_mode"),
        waterfall_scroll_speed: st.waterfall_speed,
        miditrail_z_far: st.miditrail_z_far,
        note_counter: NoteCounterConfig {
            text: st.counter_text.clone(),
            alignment: parse_enum_or_log(&st.counter_alignment, "counter_alignment"),
            font_size: st.counter_font_size,
            font: counter_font(
                &st.counter_font_mode,
                st.counter_font_family.clone(),
                st.counter_font_path.clone(),
            ),
            separator: if st.counter_use_commas {
                CounterSeparator::Comma
            } else {
                CounterSeparator::Nothing
            },
            padding_zeroes: st.counter_padding_zeroes,
            bpm_int_pad: st.counter_bpm_int_pad,
            bpm_dec_pad: st.counter_bpm_dec_pad,
            note_count_pad: st.counter_note_count_pad,
            polyphony_pad: st.counter_polyphony_pad,
            nps_pad: st.counter_nps_pad,
            ticks_pad: st.counter_ticks_pad,
            bars_pad: st.counter_bars_pad,
            frames_pad: st.counter_frames_pad,
            save_csv: st.counter_save_csv,
            csv_output: st.counter_csv_output.clone(),
            csv_format: st.counter_csv_format.clone(),
        },
        data_curve: DataCurveConfig {
            metric: parse_enum_or_log(&st.dc_metric, "dc_metric"),
            graph_duration: parse_f32_or_log(&st.dc_graph_duration, "dc_graph_duration", 2.0)
                .clamp(0.5, 30.0),
            zoom_smoothness: parse_f32_or_log(&st.dc_zoom_smoothness, "dc_zoom_smoothness", 8.0)
                .clamp(1.0, 64.0),
            graph_smoothness: parse_f32_or_log(&st.dc_graph_smoothness, "dc_graph_smoothness", 0.0)
                .clamp(0.0, 32.0) as u32,
            padding_mul: parse_f32_or_log(&st.dc_padding_mul, "dc_padding_mul", 0.1)
                .clamp(0.0, 1.0),
            bg_color: hex_to_rgba(&st.dc_bg_color).unwrap_or([0, 0, 0, 255]),
            line_color: hex_to_rgba(&st.dc_line_color).unwrap_or([0, 255, 255, 255]),
            text_color: hex_to_rgba(&st.dc_text_color).unwrap_or([255, 255, 255, 127]),
            bar_color: hex_to_rgba(&st.dc_bar_color).unwrap_or([255, 255, 255, 127]),
            line_thickness: parse_f32_or_log(&st.dc_line_thickness, "dc_line_thickness", 3.0)
                .clamp(1.0, 20.0) as u32,
            bar_thickness: parse_f32_or_log(&st.dc_bar_thickness, "dc_bar_thickness", 1.0)
                .clamp(1.0, 10.0) as u32,
            font_size: st.dc_font_size,
            font: counter_font(
                &st.dc_font_mode,
                st.dc_font_family.clone(),
                st.dc_font_path.clone(),
            ),
            text_x_offset: parse_f32_or_log(&st.dc_text_x_offset, "dc_text_x_offset", 2.0)
                .clamp(0.0, 100.0) as u32,
            text_y_offset: parse_f32_or_log(&st.dc_text_y_offset, "dc_text_y_offset", 2.0)
                .clamp(0.0, 100.0) as u32,
            milestone_scale_mul: parse_f32_or_log(
                &st.dc_milestone_scale_mul,
                "dc_milestone_scale_mul",
                1.5,
            )
            .clamp(1.0, 5.0),
            abbreviate: st.dc_abbreviate,
            abbreviate_digits: parse_f32_or_log(
                &st.dc_abbreviate_digits,
                "dc_abbreviate_digits",
                3.0,
            )
            .clamp(0.0, 10.0) as u32,
            show_text: st.dc_show_text,
            show_bars: st.dc_show_bars,
        },
    }
}

impl DialogHandler {
    pub(super) fn handle_video_export(
        &self,
        root: &mut Root,
        action: VideoExportAction,
    ) -> Option<Message> {
        use VideoExportAction as V;
        match action {
            V::OpenPanel => {
                root.sidebar.video_export_visible = true;
                root.sidebar.route = crate::sidebar::Route::VideoExport;
            }
            V::ClosePanel => {
                root.sidebar.video_export_visible = false;
                root.sidebar.route = crate::sidebar::Route::Arrangement;
            }
            V::StartExport => {
                // 内存模式优先：从 EditorData.document 克隆快照（ChunkedList Arc 共享）；
                // 否则若指定了 MIDI 路径，进入流式读取模式。
                let document = root
                    .editor
                    .editor_state
                    .data
                    .document
                    .as_ref()
                    .map(|doc| std::sync::Arc::new(doc.clone()));
                let midi_source = {
                    let st = &root.state.video_export_dialog;
                    if document.is_some() {
                        tracing::info!("视频导出使用内存 MidiDocument（零拷贝模式）");
                        "内存模式".to_string()
                    } else if !st.midi_path.is_empty() {
                        tracing::info!("视频导出使用 MIDI 文件流式读取: {}", st.midi_path);
                        "流式读取".to_string()
                    } else {
                        tracing::warn!("视频导出未配置 MIDI 数据源");
                        "未配置".to_string()
                    }
                };

                // 设置导出中状态
                root.state.video_export_dialog.overlay = VideoExportOverlayState::Exporting;
                root.state.video_export_dialog.progress = 0.0;
                root.state.video_export_dialog.status_message =
                    format!("正在初始化... [{}]", midi_source);
                root.state.video_export_dialog.current_frame = 0;
                root.state.video_export_dialog.total_frames = 0;
                root.state.video_export_dialog.render_fps = 0.0;
                root.state.video_export_dialog.cached_image_handle = None;

                let st = &root.state.video_export_dialog;
                let video_config = build_video_config(
                    st,
                    root.editor.editor_state.view.ppq,
                    root.editor.editor_state.view.visible_key_count,
                );
                let ev = crate::event::window::Event::start_video_export(video_config, document);
                crate::event::emit(crate::event::Event::Window(ev));
            }
            V::CancelExport => {
                reset_video_export_overlay(root);
                // 通知 Runner 取消导出（关闭对话框 → 设置取消标志 → 后台线程退出）
                crate::event::emit(crate::event::Event::Window(
                    crate::event::window::Event::close_video_export_dialog(),
                ));
            }
            V::ForceFinish => {
                let st = &root.state.video_export_dialog;
                root.state.video_export_dialog.overlay = VideoExportOverlayState::Completed {
                    total_frames: st.total_frames,
                    elapsed_secs: st.elapsed_secs,
                    avg_fps: st.render_fps,
                };
            }
            V::DismissOverlay => {
                reset_video_export_overlay(root);
            }
            V::ContainerChanged(v) => {
                root.state.video_export_dialog.container = v;
            }
            V::CodecChanged(v) => {
                root.state.video_export_dialog.codec = v;
            }
            V::BackendChanged(v) => {
                root.state.video_export_dialog.backend = v;
            }
            V::QualityChanged(v) => {
                root.state.video_export_dialog.quality = v;
            }
            V::WidthChanged(v) => {
                if let Some(w) = parse_uint(&v) {
                    root.state.video_export_dialog.width = w;
                }
            }
            V::HeightChanged(v) => {
                if let Some(h) = parse_uint(&v) {
                    root.state.video_export_dialog.height = h;
                }
            }
            V::RenderModeChanged(v) => {
                root.state.video_export_dialog.render_mode = v;
            }
            V::WaterfallSpeedChanged(v) => {
                root.state.video_export_dialog.waterfall_speed = v;
            }
            V::MiditrailZFarChanged(v) => {
                root.state.video_export_dialog.miditrail_z_far = v;
            }
            V::CounterTextAction(action) => {
                let st = &mut root.state.video_export_dialog;
                st.counter_editor.perform(action);
                st.counter_text = st.counter_editor.text();
            }
            V::CounterAlignmentChanged(v) => {
                root.state.video_export_dialog.counter_alignment = v;
            }
            V::CounterFontSizeChanged(v) => {
                root.state.video_export_dialog.counter_font_size = v.clamp(7, 512);
            }
            V::CounterFontModeChanged(v) => {
                root.state.video_export_dialog.counter_font_mode = v;
            }
            V::CounterFontFamilyChanged(v) => {
                root.state.video_export_dialog.counter_font_family = v;
            }
            V::CounterFontPathChanged(v) => {
                root.state.video_export_dialog.counter_font_path = v;
            }
            V::CounterBrowseFont => {
                // 浏览字体文件（TTF/OTF/TTC），选择后写入路径
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("字体文件", &["ttf", "otf", "ttc"])
                    .pick_file()
                {
                    root.state.video_export_dialog.counter_font_path =
                        path.to_string_lossy().to_string();
                }
            }
            V::CounterUseCommasChanged(v) => {
                root.state.video_export_dialog.counter_use_commas = v;
            }
            V::CounterPaddingZeroesChanged(v) => {
                root.state.video_export_dialog.counter_padding_zeroes = v;
            }
            V::CounterSaveCsvChanged(v) => {
                root.state.video_export_dialog.counter_save_csv = v;
            }
            V::CounterCsvPathChanged(v) => {
                root.state.video_export_dialog.counter_csv_output = v;
            }
            V::CounterCsvFormatChanged(v) => {
                root.state.video_export_dialog.counter_csv_format = v;
            }
            V::CounterPadChanged { field, value } => {
                let st = &mut root.state.video_export_dialog;
                match field.as_str() {
                    "bpm_int" => st.counter_bpm_int_pad = value,
                    "bpm_dec" => st.counter_bpm_dec_pad = value.min(12),
                    "nc" => st.counter_note_count_pad = value,
                    "plph" => st.counter_polyphony_pad = value,
                    "nps" => st.counter_nps_pad = value,
                    "ticks" => st.counter_ticks_pad = value,
                    "bars" => st.counter_bars_pad = value,
                    "frames" => st.counter_frames_pad = value,
                    _ => {}
                }
            }
            V::CounterResetPadding => {
                let st = &mut root.state.video_export_dialog;
                st.counter_bpm_int_pad = 3;
                st.counter_bpm_dec_pad = 2;
                st.counter_note_count_pad = 5;
                st.counter_polyphony_pad = 3;
                st.counter_nps_pad = 3;
                st.counter_ticks_pad = 5;
                st.counter_bars_pad = 3;
                st.counter_frames_pad = 5;
            }
            V::CounterLoadTemplate(name) => {
                let st = &mut root.state.video_export_dialog;
                let template = match name.as_str() {
                    "full" => crate::state::root_state::COUNTER_FULL_TEXT,
                    _ => crate::state::root_state::COUNTER_DEFAULT_TEXT,
                };
                st.counter_text = template.to_string();
                st.counter_editor = iced_widget::text_editor::Content::with_text(template);
            }
            V::CounterResetText => {
                let st = &mut root.state.video_export_dialog;
                st.counter_text = crate::state::root_state::COUNTER_DEFAULT_TEXT.to_string();
                st.counter_editor = iced_widget::text_editor::Content::with_text(&st.counter_text);
            }
            V::CounterBrowseCsv => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name("notecounter.csv")
                    .add_filter("CSV 数据文件", &["csv"])
                    .save_file()
                {
                    root.state.video_export_dialog.counter_csv_output =
                        path.to_string_lossy().to_string();
                }
            }
            V::DataCurveNumberChanged { field, value } => {
                let st = &mut root.state.video_export_dialog;
                match field.as_str() {
                    "graph_duration" => st.dc_graph_duration = fmt_num(value, 2),
                    "zoom_smoothness" => st.dc_zoom_smoothness = fmt_num(value, 2),
                    "graph_smoothness" => st.dc_graph_smoothness = fmt_num(value, 0),
                    "padding_mul" => st.dc_padding_mul = fmt_num(value, 2),
                    "line_thickness" => st.dc_line_thickness = fmt_num(value, 0),
                    "bar_thickness" => st.dc_bar_thickness = fmt_num(value, 0),
                    "text_x_offset" => st.dc_text_x_offset = fmt_num(value, 0),
                    "text_y_offset" => st.dc_text_y_offset = fmt_num(value, 0),
                    "milestone_scale_mul" => st.dc_milestone_scale_mul = fmt_num(value, 2),
                    "abbreviate_digits" => st.dc_abbreviate_digits = fmt_num(value, 0),
                    _ => {}
                }
            }
            V::DataCurveBoolChanged { field, value } => {
                let st = &mut root.state.video_export_dialog;
                match field.as_str() {
                    "abbreviate" => st.dc_abbreviate = value,
                    "show_text" => st.dc_show_text = value,
                    "show_bars" => st.dc_show_bars = value,
                    _ => {}
                }
            }
            V::DataCurveTextChanged { field, value } => {
                let st = &mut root.state.video_export_dialog;
                match field.as_str() {
                    "metric" => st.dc_metric = value,
                    "bg_color" => st.dc_bg_color = value,
                    "line_color" => st.dc_line_color = value,
                    "text_color" => st.dc_text_color = value,
                    "bar_color" => st.dc_bar_color = value,
                    "font_family" => st.dc_font_family = value,
                    "font_path" => st.dc_font_path = value,
                    _ => {}
                }
            }
            V::DataCurveFontSizeChanged(v) => {
                root.state.video_export_dialog.dc_font_size = v.clamp(7, 256);
            }
            V::DataCurveFontModeChanged(v) => {
                root.state.video_export_dialog.dc_font_mode = v;
            }
            V::DataCurveBrowseFont => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("字体文件", &["ttf", "otf", "ttc"])
                    .pick_file()
                {
                    root.state.video_export_dialog.dc_font_path =
                        path.to_string_lossy().to_string();
                }
            }
            V::FpsChanged(v) => {
                root.state.video_export_dialog.fps = v;
            }
            V::OutputPathChanged(v) => {
                root.state.video_export_dialog.output_path = v;
            }
            V::BrowseOutput => {
                let st = &root.state.video_export_dialog;
                let ext = st.container.to_lowercase();
                let default_name = format!("output.{}", ext);
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name(&default_name)
                    .add_filter(&st.container, &[ext.as_str()])
                    .save_file()
                {
                    root.state.video_export_dialog.output_path = path.to_string_lossy().to_string();
                }
            }
            V::MidiPathChanged(v) => {
                root.state.video_export_dialog.midi_path = v;
            }
            V::BrowseMidi => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("音乐文件", &["mid", "midi", "lmpj"])
                    .add_filter("MIDI 文件", &["mid", "midi"])
                    .add_filter("Lumino 项目", &["lmpj"])
                    .add_filter("所有文件", &["*"])
                    .pick_file()
                {
                    root.state.video_export_dialog.midi_path = path.to_string_lossy().to_string();
                }
            }
            V::UpdateProgress {
                message,
                progress,
                current_frame,
                total_frames,
                fps,
            } => {
                let st = &mut root.state.video_export_dialog;
                st.status_message = message;
                st.progress = progress;
                st.current_frame = current_frame;
                st.total_frames = total_frames;
                st.render_fps = fps;
            }
            V::ExportCompleted => {
                // 由 Runner 回调设置具体字段，此处不处理
            }
            V::ExportFailed(err) => {
                root.state.video_export_dialog.overlay = VideoExportOverlayState::Error(err);
            }
            V::UpdatePreviewFrame { .. } => {
                // 由 Host 直接处理，此处不需要
            }
        }
        None
    }
}
