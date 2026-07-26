//! 视频导出面板与导出覆盖层视图

use iced_core::{Alignment, Color, Length};
use iced_widget::{
    button, column, container, image, pick_list, row, scrollable, slider, space, text, text_input,
};

use crate::message::{Message, VideoExportAction};
use crate::state::root_state::{VideoExportDialogState, VideoExportOverlayState};

use super::widgets;

pub mod helpers;

/// 渲染视频导出配置面板（侧边栏面板）
pub fn view_video_export_dialog<'a>(
    state: &'a VideoExportDialogState,
    theme: &'a iced_core::Theme,
) -> crate::Element<'a> {
    let palette = theme.extended_palette();

    let main_content = column![
        title_section(palette),
        space().height(16),
        midi_source_section(state, palette),
        space().height(16),
        render_settings_section(state, palette),
        space().height(16),
        output_path_section(state, palette),
        space().height(24),
        buttons_section(palette),
    ];

    let scrollable_content = scrollable(main_content)
        .width(Length::Fill)
        .height(Length::Fill);

    container(scrollable_content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(24)
        .style(move |_t: &iced_core::Theme| {
            container::Style::default().background(palette.background.base.color)
        })
        .into()
}

/// 标题
fn title_section<'a>(palette: &'a iced_core::theme::palette::Extended) -> crate::Element<'a> {
    text("视频导出")
        .size(18)
        .font(iced_core::Font::with_name("Microsoft YaHei"))
        .style(widgets::dialog_label_style(palette))
        .into()
}

/// 渲染设置区域（容器格式、编码器、硬件加速、质量、分辨率、帧率）
fn render_settings_section<'a>(
    state: &'a VideoExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    let containers = vec!["MP4", "MOV", "MKV", "AVI"]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
    let codecs = vec!["H.264", "H.265 / HEVC", "ProRes", "VP9", "AV1"]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
    let backends = helpers::available_backends();
    let qualities = vec!["高", "中", "低"]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
    let fps_options = vec![24u32, 30, 60, 120];

    let width_str = state.width.to_string();
    let height_str = state.height.to_string();

    column![
        text("渲染设置")
            .size(16)
            .font(iced_core::Font::with_name("Microsoft YaHei"))
            .style(widgets::dialog_label_style(palette)),
        space().height(12),
        pick_list_row(
            "渲染格式:",
            100.0,
            containers,
            Some(state.container.clone()),
            |v| Message::VideoExport(VideoExportAction::ContainerChanged(v)),
            palette,
        ),
        space().height(8),
        pick_list_row(
            "编码器:",
            100.0,
            codecs,
            Some(state.codec.clone()),
            |v| Message::VideoExport(VideoExportAction::CodecChanged(v)),
            palette,
        ),
        space().height(8),
        pick_list_row(
            "加速:",
            100.0,
            backends,
            Some(state.backend.clone()),
            |v| Message::VideoExport(VideoExportAction::BackendChanged(v)),
            palette,
        ),
        space().height(8),
        pick_list_row(
            "质量:",
            100.0,
            qualities,
            Some(state.quality.clone()),
            |v| Message::VideoExport(VideoExportAction::QualityChanged(v)),
            palette,
        ),
        space().height(8),
        pick_list_row(
            "渲染模式:",
            100.0,
            vec!["瀑布流".to_string(), "音符矩形".to_string()],
            Some(state.render_mode.clone()),
            |v| Message::VideoExport(VideoExportAction::RenderModeChanged(v)),
            palette,
        ),
        space().height(8),
        // 瀑布流滚动速度滑杆
        row![
            text("下落速度:")
                .size(14)
                .style(move |_t: &iced_core::Theme| text::Style {
                    color: Some(palette.background.neutral.text),
                })
                .width(100),
            slider(0.1..=10.0, state.waterfall_speed, |v| {
                Message::VideoExport(VideoExportAction::WaterfallSpeedChanged(v))
            })
            .step(0.1_f32)
            .width(200.0),
            text(format!("{:.1}x", state.waterfall_speed))
                .size(14)
                .style(move |_t: &iced_core::Theme| text::Style {
                    color: Some(palette.background.neutral.text),
                }),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        space().height(8),
        // 分辨率行
        row![
            text("分辨率:")
                .size(14)
                .style(widgets::dialog_label_style(palette))
                .width(100),
            container(
                text_input("1920", &width_str)
                    .on_input(|v| Message::VideoExport(VideoExportAction::WidthChanged(v)))
                    .padding([6, 10])
                    .width(Length::Fixed(80.0)),
            )
            .style(widgets::dialog_input_style(palette)),
            text("x")
                .size(14)
                .style(widgets::dialog_label_style(palette)),
            container(
                text_input("1080", &height_str)
                    .on_input(|v| Message::VideoExport(VideoExportAction::HeightChanged(v)))
                    .padding([6, 10])
                    .width(Length::Fixed(80.0)),
            )
            .style(widgets::dialog_input_style(palette)),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        pick_list_row(
            "帧率:",
            100.0,
            fps_options,
            Some(state.fps),
            |v| { Message::VideoExport(VideoExportAction::FpsChanged(v)) },
            palette
        ),
    ]
    .width(Length::Fill)
    .into()
}

/// MIDI 数据源区域（内存模式优先使用已加载工程；否则使用指定 MIDI 路径流式读取）
fn midi_source_section<'a>(
    state: &'a VideoExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    let hint = if state.midi_path.is_empty() {
        "优先使用当前工程的 MIDI 数据"
    } else {
        "使用指定 MIDI 文件流式读取"
    };

    column![
        text("MIDI 数据源")
            .size(16)
            .font(iced_core::Font::with_name("Microsoft YaHei"))
            .style(widgets::dialog_label_style(palette)),
        space().height(8),
        row![
            container(
                text(&state.midi_path)
                    .size(12)
                    .style(widgets::dialog_muted_text_style(palette))
                    .width(Length::Fill),
            )
            .width(Length::Fill)
            .style(widgets::dialog_input_style(palette)),
            space().width(8),
            button(text("浏览...").size(14))
                .on_press(Message::VideoExport(VideoExportAction::BrowseMidi))
                .padding([6, 16]),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        space().height(4),
        text(hint)
            .size(12)
            .style(widgets::dialog_muted_text_style(palette)),
    ]
    .width(Length::Fill)
    .into()
}

/// 输出路径区域
fn output_path_section<'a>(
    state: &'a VideoExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    column![
        text("导出位置")
            .size(16)
            .font(iced_core::Font::with_name("Microsoft YaHei"))
            .style(widgets::dialog_label_style(palette)),
        space().height(8),
        row![
            container(
                text_input("选择输出路径...", &state.output_path)
                    .on_input(|v| Message::VideoExport(VideoExportAction::OutputPathChanged(v)))
                    .padding([6, 10])
                    .width(Length::Fill),
            )
            .width(Length::Fill)
            .style(widgets::dialog_input_style(palette)),
            space().width(8),
            button(text("浏览...").size(14))
                .on_press(Message::VideoExport(VideoExportAction::BrowseOutput))
                .padding([6, 16]),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    ]
    .width(Length::Fill)
    .into()
}

/// 关闭/导出按钮
fn buttons_section(palette: &iced_core::theme::palette::Extended) -> crate::Element<'static> {
    row![
        button(text("关闭").size(14))
            .on_press(Message::VideoExport(VideoExportAction::ClosePanel))
            .padding([8, 32])
            .width(Length::Fixed(100.0))
            .style(widgets::dialog_button_style(
                palette.background.strong.color,
                palette.background.weak.color,
                palette.background.neutral.text,
            )),
        space().width(12),
        button(text("开始导出").size(14))
            .on_press(Message::VideoExport(VideoExportAction::StartExport))
            .padding([8, 32])
            .width(Length::Fixed(120.0))
            .style(widgets::dialog_button_style(
                palette.primary.strong.color,
                palette.primary.base.color,
                Color::WHITE,
            )),
    ]
    .align_y(Alignment::Center)
    .into()
}

// ── 覆盖层视图 ────────────────────────────────────────────

/// 渲染视频导出覆盖层（浮动 dialog 弹出样式）
pub fn view_video_export_overlay<'a>(
    state: &'a VideoExportDialogState,
    theme: &'a iced_core::Theme,
) -> Option<crate::Element<'a>> {
    if matches!(state.overlay, VideoExportOverlayState::None) {
        return None;
    }
    let palette = theme.extended_palette();

    let content: crate::Element<'a> = match &state.overlay {
        VideoExportOverlayState::Exporting => exporting_overlay(state, theme, palette),
        VideoExportOverlayState::Finalizing => finalizing_overlay(state, theme, palette),
        VideoExportOverlayState::Completed {
            total_frames,
            elapsed_secs,
            avg_fps,
        } => completed_overlay(state, *total_frames, *elapsed_secs, *avg_fps, palette),
        VideoExportOverlayState::Error(err) => error_overlay(err.clone(), palette),
        VideoExportOverlayState::None => return None,
    };

    let full = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(20)
        .style(move |_t: &iced_core::Theme| container::Style {
            background: Some(palette.background.base.color.into()),
            ..Default::default()
        });

    Some(full.into())
}

/// 导出中的覆盖层
fn exporting_overlay<'a>(
    state: &'a VideoExportDialogState,
    theme: &'a iced_core::Theme,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    let detail = helpers::render_progress_detail(state, theme);
    column![
        text("视频导出中")
            .size(16)
            .style(widgets::dialog_label_style(palette)),
        space().height(8),
        preview_area(state, palette),
        space().height(8),
        detail,
        space().height(12),
        button(text("取消导出").size(14))
            .on_press(Message::VideoExport(VideoExportAction::CancelExport))
            .padding([8, 32])
            .style(widgets::dialog_button_style(
                palette.danger.strong.color,
                palette.danger.base.color,
                Color::WHITE,
            )),
    ]
    .align_x(Alignment::Center)
    .into()
}

/// 编码中的覆盖层
fn finalizing_overlay<'a>(
    state: &'a VideoExportDialogState,
    theme: &'a iced_core::Theme,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    let detail = helpers::render_progress_detail(state, theme);
    column![
        text("视频导出中")
            .size(16)
            .style(widgets::dialog_label_style(palette)),
        space().height(8),
        preview_area(state, palette),
        space().height(8),
        text("正在完成编码...")
            .size(14)
            .style(widgets::dialog_label_style(palette)),
        space().height(4),
        detail,
        space().height(4),
        text("ffmpeg 正在封装文件，请稍候")
            .size(12)
            .style(widgets::dialog_muted_text_style(palette)),
        space().height(8),
        row![
            button(text("强制完成").size(14))
                .on_press(Message::VideoExport(VideoExportAction::ForceFinish))
                .padding([6, 16])
                .style(widgets::dialog_button_style(
                    palette.background.strong.color,
                    palette.background.weak.color,
                    palette.background.neutral.text,
                )),
            space().width(8),
            text("视频已可用，跳过等待")
                .size(12)
                .style(widgets::dialog_muted_text_style(palette)),
        ]
        .align_y(Alignment::Center),
    ]
    .align_x(Alignment::Center)
    .into()
}

/// 导出完成的覆盖层
fn completed_overlay<'a>(
    state: &'a VideoExportDialogState,
    total_frames: u64,
    elapsed_secs: f64,
    avg_fps: f64,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    let duration_secs = if state.fps > 0 {
        total_frames as f64 / state.fps as f64
    } else {
        0.0
    };
    let speedup = if elapsed_secs > 0.0 && duration_secs > 0.0 {
        duration_secs / elapsed_secs
    } else {
        0.0
    };

    column![
        text("导出完成！")
            .size(16)
            .style(move |_t: &iced_core::Theme| text::Style {
                color: Some(palette.success.strong.color),
            }),
        space().height(12),
        preview_area_empty(palette),
        space().height(8),
        text(format!("总帧数: {total_frames}"))
            .size(13)
            .style(widgets::dialog_muted_text_style(palette)),
        text(format!("时长: {}", helpers::format_duration(duration_secs)))
            .size(13)
            .style(widgets::dialog_muted_text_style(palette)),
        text(format!(
            "总用时: {}",
            helpers::format_duration(elapsed_secs)
        ))
        .size(13)
        .style(widgets::dialog_muted_text_style(palette)),
        text(format!("平均速度: {avg_fps:.1} fps"))
            .size(13)
            .style(widgets::dialog_muted_text_style(palette)),
        text(format!("倍率: {speedup:.1}x 原速"))
            .size(13)
            .style(widgets::dialog_muted_text_style(palette)),
        space().height(16),
        button(text("确定").size(14))
            .on_press(Message::VideoExport(VideoExportAction::DismissOverlay))
            .padding([8, 32])
            .style(widgets::dialog_button_style(
                palette.primary.strong.color,
                palette.primary.base.color,
                Color::WHITE,
            )),
    ]
    .align_x(Alignment::Center)
    .into()
}

/// 导出失败的覆盖层
fn error_overlay<'a>(
    err: String,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    column![
        text("导出失败")
            .size(16)
            .style(move |_t: &iced_core::Theme| text::Style {
                color: Some(palette.danger.strong.color),
            }),
        space().height(12),
        container(
            scrollable(
                text(err)
                    .size(13)
                    .style(move |_t: &iced_core::Theme| text::Style {
                        color: Some(palette.danger.weak.color),
                    })
            )
            .height(Length::Fixed(200.0))
        )
        .width(Length::Fill),
        space().height(16),
        button(text("确定").size(14))
            .on_press(Message::VideoExport(VideoExportAction::DismissOverlay))
            .padding([8, 32])
            .style(widgets::dialog_button_style(
                palette.primary.strong.color,
                palette.primary.base.color,
                Color::WHITE,
            )),
    ]
    .align_x(Alignment::Center)
    .into()
}

// ── 共享辅助函数 ───────────────────────────────────────────

/// pick_list 选择行
fn pick_list_row<'a, T: 'a + Clone + ToString + PartialEq>(
    label: &'a str,
    label_width: f32,
    options: Vec<T>,
    selected: Option<T>,
    on_selected: impl Fn(T) -> Message + 'a,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    let label_color = palette.background.neutral.text;
    let label_style = move |_t: &iced_core::Theme| text::Style {
        color: Some(label_color),
    };
    row![
        text(label).size(14).style(label_style).width(label_width),
        pick_list(options, selected, on_selected).width(Length::Fixed(200.0)),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

/// 预览区域（有缓存图片时）
fn preview_area<'a>(
    state: &'a VideoExportDialogState,
    palette: &'a iced_core::theme::palette::Extended,
) -> crate::Element<'a> {
    if let Some(ref handle) = state.cached_image_handle {
        let preview_max_w = 480.0;
        let preview_max_h = 240.0;
        let img_w = state.preview_width as f32;
        let img_h = state.preview_height as f32;
        let scale = (preview_max_w / img_w).min(preview_max_h / img_h).min(1.0);
        let display_w = (img_w * scale).max(100.0);
        let display_h = (img_h * scale).max(56.0);

        container(image(handle).width(display_w).height(display_h))
            .width(Length::Fill)
            .center_x(Length::Fill)
            .style(move |_t: &iced_core::Theme| container::Style {
                background: Some(palette.background.weak.color.into()),
                border: iced_core::Border {
                    radius: 4.0.into(),
                    width: 1.0,
                    color: palette.background.strong.color,
                },
                ..Default::default()
            })
            .into()
    } else {
        preview_area_empty(palette)
    }
}

/// 预览区域（无图片时）
fn preview_area_empty<'a>(palette: &'a iced_core::theme::palette::Extended) -> crate::Element<'a> {
    container(
        text("等待渲染...")
            .size(14)
            .style(widgets::dialog_muted_text_style(palette)),
    )
    .width(Length::Fill)
    .height(Length::Fixed(120.0))
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .style(move |_t: &iced_core::Theme| container::Style {
        background: Some(palette.background.weak.color.into()),
        border: iced_core::Border {
            radius: 4.0.into(),
            width: 1.0,
            color: palette.background.strong.color,
        },
        ..Default::default()
    })
    .into()
}
