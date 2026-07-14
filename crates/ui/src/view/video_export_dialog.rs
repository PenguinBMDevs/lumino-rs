//! 视频导出面板与导出覆盖层视图
//!
//! 配置面板参照 audio_export_dialog 的 iced 风格。
//! 导出覆盖层参照 nezha export_controller 的模态遮罩 + 居中窗口。

use iced_core::{Alignment, Color, Length};
use iced_widget::{
    button, column, container, image, pick_list, progress_bar, row, scrollable, space, text,
    text_input,
};

use crate::message::{Message, VideoExportAction};
use crate::state::root_state::{VideoExportDialogState, VideoExportOverlayState};

/// 渲染视频导出配置面板（侧边栏面板）
pub fn view_video_export_dialog<'a>(
    state: &'a VideoExportDialogState,
    theme: &'a iced_core::Theme,
) -> crate::Element<'a> {
    let palette = theme.extended_palette();

    let label_style = move |_theme: &iced_core::Theme| text::Style {
        color: Some(palette.background.neutral.text),
    };

    let input_style = move |_theme: &iced_core::Theme| container::Style {
        background: Some(palette.background.weak.color.into()),
        border: iced_core::Border {
            radius: 4.0.into(),
            width: 1.0,
            color: palette.background.strong.color,
        },
        ..Default::default()
    };

    let title = text("视频导出")
        .size(18)
        .font(iced_core::Font::with_name("Microsoft YaHei"))
        .style(label_style);

    // ── 渲染设置区域 ──
    let containers = vec!["MP4", "MOV", "MKV", "AVI"]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
    let codecs = vec!["H.264", "H.265 / HEVC", "ProRes", "VP9", "AV1"]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
    let backends = available_backends();
    let qualities = vec!["高", "中", "低"]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
    let fps_options = vec![24u32, 30, 60, 120];

    let render_settings = column![
        text("渲染设置")
            .size(16)
            .font(iced_core::Font::with_name("Microsoft YaHei"))
            .style(label_style),
        space().height(12),
        row![
            text("渲染格式:").size(14).style(label_style).width(100),
            pick_list(containers, Some(state.container.clone()), |v| {
                Message::VideoExport(VideoExportAction::ContainerChanged(v))
            },)
            .width(Length::Fixed(200.0)),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        space().height(8),
        row![
            text("编码器:").size(14).style(label_style).width(100),
            pick_list(codecs, Some(state.codec.clone()), |v| Message::VideoExport(
                VideoExportAction::CodecChanged(v)
            ),)
            .width(Length::Fixed(200.0)),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        space().height(8),
        row![
            text("加速:").size(14).style(label_style).width(100),
            pick_list(backends, Some(state.backend.clone()), |v| {
                Message::VideoExport(VideoExportAction::BackendChanged(v))
            },)
            .width(Length::Fixed(200.0)),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        space().height(8),
        row![
            text("质量:").size(14).style(label_style).width(100),
            pick_list(qualities, Some(state.quality.clone()), |v| {
                Message::VideoExport(VideoExportAction::QualityChanged(v))
            },)
            .width(Length::Fixed(200.0)),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        space().height(8),
        // 分辨率
        row![
            text("分辨率:").size(14).style(label_style).width(100),
            container(
                text_input("1920", &state.width.to_string())
                    .on_input(|v| Message::VideoExport(VideoExportAction::WidthChanged(v)))
                    .padding([6, 10])
                    .width(Length::Fixed(80.0)),
            )
            .style(input_style),
            text("x").size(14).style(label_style),
            container(
                text_input("1080", &state.height.to_string())
                    .on_input(|v| Message::VideoExport(VideoExportAction::HeightChanged(v)))
                    .padding([6, 10])
                    .width(Length::Fixed(80.0)),
            )
            .style(input_style),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        space().height(8),
        // 帧率
        row![
            text("帧率:").size(14).style(label_style).width(100),
            pick_list(fps_options, Some(state.fps), |v| Message::VideoExport(
                VideoExportAction::FpsChanged(v)
            ),)
            .width(Length::Fixed(200.0)),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        // 视频导出渲染模式
        space().height(8),
        {
            use lumino_event::window::video::RenderMode;
            let render_modes = vec![RenderMode::NoteRectangle, RenderMode::HiResTexture];
            row![
                text("渲染模式:").size(14).style(label_style).width(100),
                pick_list(render_modes, Some(state.render_mode), |v| {
                    Message::VideoExport(VideoExportAction::RenderModeChanged(v))
                })
                .width(Length::Fixed(200.0)),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
        },
    ]
    .width(Length::Fill);

    // ── 输出路径区域 ──
    let output_path = column![
        text("导出位置")
            .size(16)
            .font(iced_core::Font::with_name("Microsoft YaHei"))
            .style(label_style),
        space().height(8),
        row![
            container(
                text_input("选择输出路径...", &state.output_path)
                    .on_input(|v| Message::VideoExport(VideoExportAction::OutputPathChanged(v)))
                    .padding([6, 10])
                    .width(Length::Fill),
            )
            .width(Length::Fill)
            .style(input_style),
            space().width(8),
            button(text("浏览...").size(14))
                .on_press(Message::VideoExport(VideoExportAction::BrowseOutput))
                .padding([6, 16]),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    ]
    .width(Length::Fill);

    // ── 按钮区域 ──
    let buttons = row![
        button(text("关闭").size(14))
            .on_press(Message::VideoExport(VideoExportAction::ClosePanel))
            .padding([8, 32])
            .width(Length::Fixed(100.0))
            .style(move |_t: &iced_core::Theme, status| {
                let bg = match status {
                    button::Status::Hovered => palette.background.strong.color,
                    _ => palette.background.weak.color,
                };
                button::Style {
                    background: Some(bg.into()),
                    text_color: palette.background.neutral.text,
                    border: iced_core::Border {
                        radius: 4.0.into(),
                        width: 0.0,
                        color: Color::TRANSPARENT,
                    },
                    ..Default::default()
                }
            }),
        space().width(12),
        button(text("开始导出").size(14))
            .on_press(Message::VideoExport(VideoExportAction::StartExport))
            .padding([8, 32])
            .width(Length::Fixed(120.0))
            .style(move |_t: &iced_core::Theme, status| {
                let bg = match status {
                    button::Status::Hovered => palette.primary.strong.color,
                    _ => palette.primary.base.color,
                };
                button::Style {
                    background: Some(bg.into()),
                    text_color: Color::WHITE,
                    border: iced_core::Border {
                        radius: 4.0.into(),
                        width: 0.0,
                        color: Color::TRANSPARENT,
                    },
                    ..Default::default()
                }
            }),
    ]
    .align_y(Alignment::Center);

    let main_content = column![
        title,
        space().height(16),
        render_settings,
        space().height(16),
        output_path,
        space().height(24),
        buttons,
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

/// 渲染视频导出覆盖层（浮动 dialog 弹出样式，使用 Stack 层叠在配置面板上方）
pub fn view_video_export_overlay<'a>(
    state: &'a VideoExportDialogState,
    theme: &'a iced_core::Theme,
) -> Option<crate::Element<'a>> {
    if matches!(state.overlay, VideoExportOverlayState::None) {
        return None;
    }

    let palette = theme.extended_palette();

    let label_style = move |_theme: &iced_core::Theme| text::Style {
        color: Some(palette.background.neutral.text),
    };

    let weak_text = move |_theme: &iced_core::Theme| text::Style {
        color: Some(palette.background.weak.text),
    };

    // ── 预览图像区域 ──
    let preview_area: crate::Element<'a> = if let Some(ref handle) = state.cached_image_handle {
        // 使用缓存的 handle（相同数据复用已上传的 GPU 纹理，避免每帧重新异步上传）
        // 预览区域宽度基于 dialog 窗口尺寸（520x560）
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
        container(text("等待渲染...").size(14).style(weak_text))
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
    };

    let content: crate::Element<'a> = match &state.overlay {
        VideoExportOverlayState::Exporting => {
            let detail = render_progress_detail(state, theme);
            column![
                text("视频导出中").size(16).style(label_style),
                space().height(8),
                preview_area,
                space().height(8),
                detail,
                space().height(12),
                button(text("取消导出").size(14))
                    .on_press(Message::VideoExport(VideoExportAction::CancelExport))
                    .padding([8, 32])
                    .style(move |_t: &iced_core::Theme, status| {
                        let bg = match status {
                            button::Status::Hovered => palette.danger.strong.color,
                            _ => palette.danger.base.color,
                        };
                        button::Style {
                            background: Some(bg.into()),
                            text_color: Color::WHITE,
                            border: iced_core::Border {
                                radius: 4.0.into(),
                                width: 0.0,
                                color: Color::TRANSPARENT,
                            },
                            ..Default::default()
                        }
                    }),
            ]
            .align_x(Alignment::Center)
            .into()
        }
        VideoExportOverlayState::Finalizing => {
            let detail = render_progress_detail(state, theme);
            column![
                text("视频导出中").size(16).style(label_style),
                space().height(8),
                preview_area,
                space().height(8),
                text("正在完成编码...").size(14).style(label_style),
                space().height(4),
                detail,
                space().height(4),
                text("ffmpeg 正在封装文件，请稍候")
                    .size(12)
                    .style(weak_text),
                space().height(8),
                row![
                    button(text("强制完成").size(14))
                        .on_press(Message::VideoExport(VideoExportAction::ForceFinish))
                        .padding([6, 16])
                        .style(move |_t: &iced_core::Theme, status| {
                            let bg = match status {
                                button::Status::Hovered => palette.background.strong.color,
                                _ => palette.background.weak.color,
                            };
                            button::Style {
                                background: Some(bg.into()),
                                text_color: palette.background.neutral.text,
                                border: iced_core::Border {
                                    radius: 4.0.into(),
                                    width: 0.0,
                                    color: Color::TRANSPARENT,
                                },
                                ..Default::default()
                            }
                        }),
                    space().width(8),
                    text("视频已可用，跳过等待").size(12).style(weak_text),
                ]
                .align_y(Alignment::Center),
            ]
            .align_x(Alignment::Center)
            .into()
        }
        VideoExportOverlayState::Completed {
            total_frames,
            elapsed_secs,
            avg_fps,
        } => {
            let duration_secs = if state.fps > 0 {
                *total_frames as f64 / state.fps as f64
            } else {
                0.0
            };
            let speedup = if *elapsed_secs > 0.0 && duration_secs > 0.0 {
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
                preview_area,
                space().height(8),
                text(format!("总帧数: {}", total_frames))
                    .size(13)
                    .style(weak_text),
                text(format!("时长: {}", format_duration(duration_secs)))
                    .size(13)
                    .style(weak_text),
                text(format!("总用时: {}", format_duration(*elapsed_secs)))
                    .size(13)
                    .style(weak_text),
                text(format!("平均速度: {:.1} fps", avg_fps))
                    .size(13)
                    .style(weak_text),
                text(format!("倍率: {:.1}x 原速", speedup))
                    .size(13)
                    .style(weak_text),
                space().height(16),
                button(text("确定").size(14))
                    .on_press(Message::VideoExport(VideoExportAction::DismissOverlay))
                    .padding([8, 32])
                    .style(move |_t: &iced_core::Theme, status| {
                        let bg = match status {
                            button::Status::Hovered => palette.primary.strong.color,
                            _ => palette.primary.base.color,
                        };
                        button::Style {
                            background: Some(bg.into()),
                            text_color: Color::WHITE,
                            border: iced_core::Border {
                                radius: 4.0.into(),
                                width: 0.0,
                                color: Color::TRANSPARENT,
                            },
                            ..Default::default()
                        }
                    }),
            ]
            .align_x(Alignment::Center)
            .into()
        }
        VideoExportOverlayState::Error(err) => column![
            text("导出失败")
                .size(16)
                .style(move |_t: &iced_core::Theme| text::Style {
                    color: Some(palette.danger.strong.color),
                }),
            space().height(12),
            container(
                scrollable(text(err).size(13).style(move |_t: &iced_core::Theme| {
                    text::Style {
                        color: Some(palette.danger.weak.color),
                    }
                }))
                .height(Length::Fixed(200.0))
            )
            .width(Length::Fill),
            space().height(16),
            button(text("确定").size(14))
                .on_press(Message::VideoExport(VideoExportAction::DismissOverlay))
                .padding([8, 32])
                .style(move |_t: &iced_core::Theme, status| {
                    let bg = match status {
                        button::Status::Hovered => palette.primary.strong.color,
                        _ => palette.primary.base.color,
                    };
                    button::Style {
                        background: Some(bg.into()),
                        text_color: Color::WHITE,
                        border: iced_core::Border {
                            radius: 4.0.into(),
                            width: 0.0,
                            color: Color::TRANSPARENT,
                        },
                        ..Default::default()
                    }
                }),
        ]
        .align_x(Alignment::Center)
        .into(),
        VideoExportOverlayState::None => return None,
    };

    // 直接铺满整个 dialog 窗口，去掉多余的嵌套框框
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

/// 渲染进度详情（帧数/时间/进度条/速度/倍率/已用剩余）
fn render_progress_detail<'a>(
    state: &'a VideoExportDialogState,
    theme: &'a iced_core::Theme,
) -> crate::Element<'a> {
    let palette = theme.extended_palette();
    let weak_text = move |_t: &iced_core::Theme| text::Style {
        color: Some(palette.background.weak.text),
    };

    let fps = state.fps.max(1) as f64;
    let current_sec = state.current_frame as f64 / fps;
    let total_sec = state.total_frames as f64 / fps;
    let speedup = if current_sec > 0.0 && state.render_fps > 0.0 {
        current_sec / (state.current_frame as f64 / state.render_fps)
    } else {
        0.0
    };

    let elapsed_str = if state.current_frame > 0 && state.render_fps > 0.0 {
        let elapsed = state.current_frame as f64 / state.render_fps;
        let remaining = (state.total_frames - state.current_frame) as f64 / state.render_fps;
        format!(
            "已用: {} / 剩余: {}",
            format_duration(elapsed),
            format_duration(remaining)
        )
    } else {
        format!("已用: {}", format_duration(0.0))
    };

    column![
        text(format!(
            "帧: {} / {}",
            state.current_frame, state.total_frames
        ))
        .size(13)
        .style(weak_text),
        text(format!(
            "时间: {} / {}",
            format_duration(current_sec),
            format_duration(total_sec)
        ))
        .size(13)
        .style(weak_text),
        space().height(4),
        progress_bar(0.0..=1.0, state.progress as f32),
        space().height(4),
        text(format!("{:.1}%", state.progress * 100.0))
            .size(12)
            .style(weak_text),
        space().height(8),
        text(format!("渲染速度: {:.1} fps", state.render_fps))
            .size(13)
            .style(weak_text),
        text(format!("速度: {:.1}x 原速", speedup))
            .size(13)
            .style(weak_text),
        text(elapsed_str).size(13).style(weak_text),
    ]
    .width(Length::Fill)
    .into()
}

/// 格式化时长（秒 → "M:SSS.S" 或 "H:MM:SSS.S"）
fn format_duration(secs: f64) -> String {
    if secs <= 0.0 {
        return "0:00.0".to_string();
    }
    let total_tenths = (secs * 10.0) as u64;
    let tenths = total_tenths % 10;
    let total_secs = total_tenths / 10;
    let s = total_secs % 60;
    let total_mins = total_secs / 60;
    let m = total_mins % 60;
    let h = total_mins / 60;
    if h > 0 {
        format!("{}:{}:{:02}.{}", h, m, s, tenths)
    } else {
        format!("{}:{:02}.{}", m, s, tenths)
    }
}

/// 返回当前平台可用的加速后端列表
fn available_backends() -> Vec<String> {
    let mut list = vec!["Software (CPU)".to_string()];
    #[cfg(target_os = "macos")]
    list.push("VideoToolbox (macOS)".to_string());
    #[cfg(target_os = "windows")]
    {
        list.push("NVENC (NVIDIA)".to_string());
        list.push("AMF (AMD)".to_string());
        list.push("QSV (Intel)".to_string());
    }
    #[cfg(target_os = "linux")]
    {
        list.push("NVENC (NVIDIA)".to_string());
        list.push("QSV (Intel)".to_string());
        list.push("VAAPI (Linux)".to_string());
    }
    list
}
