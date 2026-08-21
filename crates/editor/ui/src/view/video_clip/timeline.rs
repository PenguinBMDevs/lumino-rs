//! 剪辑带时间轴（视频/音频轨道）
//!
//! 视频带长度 = MIDI 实际时长（total_ticks → 秒），随编辑动态变化；
//! 音频带与视频带等长。时间轴横向可滚动，标尺显示秒刻度。

use iced_core::{Color, Length};
use iced_widget::{column, container, row, scrollable, text};

use crate::Theme;

/// 将 tick 转换为秒（与 `video_export.rs` 的 `ticks_to_seconds` 一致）
fn ticks_to_seconds(tick: u64, ppq: u32, tempos: &[(u32, f32)]) -> f64 {
    if ppq == 0 {
        return tick as f64;
    }
    let mut total_secs = 0.0;
    let mut prev_tick: u32 = 0;
    let mut prev_bpm: f32 = 120.0;
    for &(t, bpm) in tempos {
        let segment_ticks = t.saturating_sub(prev_tick) as u64;
        let segment_secs = segment_ticks as f64 * 60.0 / (prev_bpm as f64 * ppq as f64);
        total_secs += segment_secs;
        if tick <= t as u64 {
            let within_ticks = tick.saturating_sub(prev_tick as u64);
            let within_secs = within_ticks as f64 * 60.0 / (prev_bpm as f64 * ppq as f64);
            return total_secs - segment_secs + within_secs;
        }
        prev_tick = t;
        prev_bpm = bpm;
    }
    let remaining = tick.saturating_sub(prev_tick as u64);
    total_secs + remaining as f64 * 60.0 / (prev_bpm as f64 * ppq as f64)
}

/// 渲染剪辑带时间轴
///
/// `total_ticks` 来自 `editor.view.total_ticks`（随编辑动态更新），
/// `ppq` 来自 `view.ppq`，`tempos` 来自 `data.tempo_points` 转换。
pub fn timeline_pane(
    theme: &Theme,
    total_ticks: u32,
    ppq: u16,
    tempos: &[(u32, f32)],
) -> crate::Element<'static> {
    let palette = theme.extended_palette();
    let weak_text = palette.background.weak.text;
    let strong_text = palette.background.strong.text;
    let weak_color = palette.background.weak.color;
    let strong_color = palette.background.strong.color;
    let weakest_color = palette.background.weakest.color;

    // 计算 MIDI 实际时长（秒）
    let duration_secs = if total_ticks == 0 {
        // 空工程默认 4 小节（与 ViewState::DEFAULT_TOTAL_TICKS 对应约 8 秒@120BPM）
        ticks_to_seconds(
            lumino_core::view_state::DEFAULT_TOTAL_TICKS as u64,
            ppq as u32,
            tempos,
        )
        .max(2.0)
    } else {
        ticks_to_seconds(total_ticks as u64, ppq as u32, tempos).max(1.0)
    };

    // 像素密度：80px/秒，随时长动态计算总宽度
    let pixels_per_sec: f32 = 80.0;
    let total_width = (duration_secs as f32 * pixels_per_sec).max(400.0);
    let ruler_height = 24.0;
    let track_height = 48.0;
    let track_spacing = 8.0;

    // 标尺：每 5 秒一个主刻度，每 1 秒一个次刻度
    let mut ruler_ticks: Vec<crate::Element<'_>> = Vec::new();
    let major_interval = 5.0;
    let minor_interval = 1.0;
    let mut t = 0.0;
    while t <= duration_secs {
        let is_major = (t % major_interval).abs() < 0.01;
        let tick_h = if is_major { 12.0 } else { 6.0 };
        let label = if is_major {
            format!("{:.0}s", t)
        } else {
            String::new()
        };
        let tick_col = column![
            container(text(label).size(10).style(move |_t: &Theme| text::Style {
                color: Some(weak_text)
            }))
            .width(Length::Fixed(30.0))
            .center_x(Length::Fixed(30.0)),
            container(
                iced_widget::space()
                    .width(Length::Fixed(1.0))
                    .height(Length::Fixed(tick_h))
            )
            .width(Length::Fixed(1.0))
            .height(Length::Fixed(tick_h))
            .style(move |_t: &iced_core::Theme| container::Style {
                background: Some(if is_major {
                    strong_color.into()
                } else {
                    weak_color.into()
                }),
                ..Default::default()
            }),
        ]
        .width(Length::Fixed(30.0))
        .align_x(iced_core::Alignment::Center)
        .spacing(2);

        // 使用绝对定位：通过 row + space 模拟 x 偏移（简化版：直接按顺序排列，间隔 = pixels_per_sec）
        // 为保持简单，标尺直接用 row 排列，间隔由 spacer 控制
        ruler_ticks.push(
            container(tick_col)
                .width(Length::Fixed(30.0))
                .center_x(Length::Fixed(30.0))
                .into(),
        );
        t += minor_interval;
        // 避免无限循环（duration 可能很大）
        if ruler_ticks.len() > 200 {
            break;
        }
    }

    // 标尺行：横向 row，宽度 = total_width
    let ruler_row = container(
        row(ruler_ticks)
            .spacing((pixels_per_sec - 30.0).max(0.0))
            .align_y(iced_core::Alignment::End),
    )
    .width(Length::Fixed(total_width))
    .height(Length::Fixed(ruler_height))
    .style(move |_t: &iced_core::Theme| container::Style {
        background: Some(weakest_color.into()),
        border: iced_core::Border {
            color: strong_color,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    });

    // 视频轨道条（蓝色系）
    let video_color = Color::from_rgb(0.2, 0.6, 0.95);
    let video_bar =
        container(
            row![
                container(text("视频").size(11).style(move |_t: &Theme| text::Style {
                    color: Some(Color::WHITE)
                }))
                .width(Length::Fixed(40.0))
                .center_y(Length::Fill),
                container(text(format!("{:.1}s", duration_secs)).size(10).style(
                    move |_t: &Theme| text::Style {
                        color: Some(Color::WHITE)
                    }
                ))
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .width(Length::Fill)
            ]
            .align_y(iced_core::Alignment::Center)
            .padding([4, 8])
            .spacing(8),
        )
        .width(Length::Fixed(total_width))
        .height(Length::Fixed(track_height))
        .style(move |_t: &iced_core::Theme| container::Style {
            background: Some(video_color.into()),
            border: iced_core::Border {
                color: Color::from_rgb(0.15, 0.45, 0.75),
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        });

    // 音频轨道条（绿色系，与视频等长）
    let audio_color = Color::from_rgb(0.25, 0.75, 0.35);
    let audio_bar =
        container(
            row![
                container(text("音频").size(11).style(move |_t: &Theme| text::Style {
                    color: Some(Color::WHITE)
                }))
                .width(Length::Fixed(40.0))
                .center_y(Length::Fill),
                container(text(format!("{:.1}s", duration_secs)).size(10).style(
                    move |_t: &Theme| text::Style {
                        color: Some(Color::WHITE)
                    }
                ))
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .width(Length::Fill)
            ]
            .align_y(iced_core::Alignment::Center)
            .padding([4, 8])
            .spacing(8),
        )
        .width(Length::Fixed(total_width))
        .height(Length::Fixed(track_height))
        .style(move |_t: &iced_core::Theme| container::Style {
            background: Some(audio_color.into()),
            border: iced_core::Border {
                color: Color::from_rgb(0.2, 0.6, 0.3),
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        });

    // 轨道列
    let tracks_col = column![
        ruler_row,
        iced_widget::space().height(track_spacing),
        video_bar,
        iced_widget::space().height(track_spacing),
        audio_bar,
    ]
    .width(Length::Fixed(total_width))
    .spacing(0);

    // 横向可滚动
    let scrollable_content = scrollable(tracks_col)
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new().width(8).scroller_width(8),
        ))
        .width(Length::Fill)
        .height(Length::Fill);

    container(
        column![
            // 标题行
            row![
                text("剪辑带")
                    .size(12)
                    .style(move |_t: &Theme| text::Style {
                        color: Some(strong_text)
                    }),
                iced_widget::space().width(Length::Fill),
                text(format!(
                    "时长 {:.1}s  ({} ticks)",
                    duration_secs, total_ticks
                ))
                .size(11)
                .style(move |_t: &Theme| text::Style {
                    color: Some(weak_text)
                }),
            ]
            .align_y(iced_core::Alignment::Center)
            .padding([4, 8]),
            scrollable_content,
        ]
        .spacing(4)
        .width(Length::Fill)
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fixed(
        crate::view::video_clip::layout::TIMELINE_HEIGHT,
    ))
    .padding(8)
    .style(move |_t: &iced_core::Theme| container::Style {
        background: Some(weak_color.into()),
        border: iced_core::Border {
            color: strong_color,
            width: 1.0,
            radius: 6.0.into(),
        },
        ..Default::default()
    })
    .into()
}
