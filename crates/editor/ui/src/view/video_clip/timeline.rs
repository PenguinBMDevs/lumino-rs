//! 视频剪辑剪辑带时间轴（组装层）
//!
//! 结构对标钢琴卷帘：内容区（[`TimelineCanvas`] 自绘）+ 底部
//! [`ScrollbarWidget`] 滚动条行（滑块拖拽滚动、边缘拖拽缩放，卷帘同款样式）。

use iced_core::Length;
use iced_widget::{column, container, row, text};
use lumino_ui_core::state::video_clip_state::{ClipTrackEdit, VideoClipState};

use crate::resources::icon::{self, Icon};
use crate::Theme;

/// 将 tick 转换为秒（与 `video_export.rs` 的 `ticks_to_seconds` 一致）。
///
/// 剪辑带全链路共用：时长计算（[`duration_seconds`]）、播放头秒数换算
/// （panels/frame 走带联动）均经此单一实现，保证标尺/轨道条/走带线同源。
pub(crate) fn ticks_to_seconds(tick: u64, ppq: u32, tempos: &[(u32, f32)]) -> f64 {
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

/// 计算 MIDI 实际时长（秒），空工程回退默认时长
pub(crate) fn duration_seconds(total_ticks: u32, ppq: u16, tempos: &[(u32, f32)]) -> f64 {
    if total_ticks == 0 {
        // 空工程默认时长（与 ViewState::DEFAULT_TOTAL_TICKS 对应约 8 秒@120BPM）
        ticks_to_seconds(
            lumino_core::view_state::DEFAULT_TOTAL_TICKS as u64,
            ppq as u32,
            tempos,
        )
        .max(2.0)
    } else {
        ticks_to_seconds(total_ticks as u64, ppq as u32, tempos).max(1.0)
    }
}

/// 剪辑带时间轴视图参数（打包传入，避免长参数列表）
pub struct TimelinePaneParams<'a> {
    /// 内容总长（tick，调用方传入真实轨尾标，见 `Root::clip_real_total_ticks`）
    pub total_ticks: u32,
    /// PPQ（每四分音符 tick 数）
    pub ppq: u16,
    /// Tempo 映射 `(tick, bpm)`
    pub tempos: &'a [(u32, f32)],
    /// 剪辑视口状态（zoom 与 timeline_scroll_x 驱动内容与滚动条）
    pub clip: &'a VideoClipState,
    /// 时间轴可视宽度（由面板 responsive 计算传入）
    pub viewport_w: f32,
    /// Ctrl 键状态（Ctrl+滚轮缩放）
    pub ctrl_pressed: bool,
    /// 当前播放位置（秒）；走带线画在其对应屏幕坐标，
    /// 播放中因滚动自动跟随恒定钉在 [`layout::PLAYHEAD_X`](super::layout::PLAYHEAD_X)
    pub playhead_secs: f32,
    /// 是否正在播放（播放中禁用标尺点击/拖拽定位）
    pub is_playing: bool,
    /// 视频轨素材编辑（偏移/首尾裁剪）
    pub video_edit: ClipTrackEdit,
    /// 音频轨素材编辑（偏移/首尾裁剪）
    pub audio_edit: ClipTrackEdit,
}

/// 渲染剪辑带时间轴
pub fn timeline_pane(theme: &Theme, params: TimelinePaneParams<'_>) -> crate::Element<'static> {
    let palette = theme.extended_palette();
    let weak_text = palette.background.weak.text;
    let strong_text = palette.background.strong.text;
    let weak_color = palette.background.weak.color;
    let strong_color = palette.background.strong.color;

    let duration_secs = duration_seconds(params.total_ticks, params.ppq, params.tempos) as f32;

    let canvas_data = super::timeline_canvas::TimelineCanvas {
        duration_secs,
        zoom: params.clip.zoom,
        scroll_x: params.clip.timeline_scroll_x,
        ctrl_pressed: params.ctrl_pressed,
        playhead_secs: params.playhead_secs,
        is_playing: params.is_playing,
        video_edit: params.video_edit,
        audio_edit: params.audio_edit,
    };
    let content_width = canvas_data.content_width();

    // 内容区 Canvas（标尺 + 双轨 + 走带指示线）
    let timeline_canvas = iced_widget::canvas::Canvas::new(canvas_data)
        .width(Length::Fill)
        .height(Length::Fill);
    // 剪辑面板独立传输按钮（与卷帘 PlaybackManager 完全无关）
    // 以 SVG 图标渲染（复用 app 级图标管线，禁止 emoji）
    let play_icon = if params.is_playing {
        Icon::Pause
    } else {
        Icon::Play
    };
    let play_btn = iced_widget::button(icon::view_with_size_and_theme(
        play_icon,
        16,
        16,
        Some(theme),
    ))
    .on_press(Message::VideoClip(VideoClipAction::ClipPlayToggled))
    .padding([2, 6]);
    let rewind_btn = iced_widget::button(icon::view_with_size_and_theme(
        Icon::SkipBackward,
        16,
        16,
        Some(theme),
    ))
    .on_press(Message::VideoClip(VideoClipAction::ClipRewound))
    .padding([2, 6]);

    // 卷帘同款水平滚动条：滑块拖拽滚动 + 边缘拖拽缩放（视口宽随消息携带，无状态反向同步）
    use crate::message::{Message, VideoClipAction};
    let viewport_w = params.viewport_w;
    let h_scrollbar = crate::editor::scrollbar_widget::ScrollbarWidget::horizontal(
        params.clip.timeline_scroll_x,
        content_width,
        params.clip.zoom,
        Some(viewport_w.max(1.0)),
        move |scroll| {
            Message::VideoClip(VideoClipAction::TimelineScroll {
                x: scroll,
                viewport_w,
            })
        },
        move |zoom, ratio| {
            Message::VideoClip(VideoClipAction::TimelineZoom {
                zoom,
                fixed_ratio: ratio,
                viewport_w,
            })
        },
    );

    container(
        column![
            // 标题行：独立传输按钮 + 时长/位置信息
            row![
                text("剪辑带")
                    .size(12)
                    .style(move |_t: &Theme| text::Style {
                        color: Some(strong_text)
                    }),
                rewind_btn,
                play_btn,
                text(format!(
                    "时长 {:.1}s  位置 {:.1}s  缩放 {:.1}x",
                    duration_secs, params.playhead_secs, params.clip.zoom
                ))
                .size(11)
                .style(move |_t: &Theme| text::Style {
                    color: Some(weak_text)
                }),
            ]
            .align_y(iced_core::Alignment::Center)
            .spacing(4)
            .padding([4, 8]),
            timeline_canvas,
            h_scrollbar,
        ]
        .spacing(4)
        .width(Length::Fill)
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fixed(super::layout::TIMELINE_HEIGHT))
    .padding(8)
    .style(move |_t: &iced_core::Theme| iced_widget::container::Style {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_duration_empty_project_fallback() {
        // 空工程 → 默认 4 小节 @120BPM/ppq=480 ≈ 8s
        let d = duration_seconds(0, 480, &[]);
        assert!(d >= 2.0, "空工程兜底时长应 ≥2s，实际 {d}");
    }

    #[test]
    fn test_duration_constant_tempo() {
        // 960 ticks @ 120BPM、ppq=480 = 1 秒
        let d = duration_seconds(960, 480, &[]);
        assert!((d - 1.0).abs() < 0.001, "960 ticks 应为 1s，实际 {d}");
    }

    #[test]
    fn test_duration_with_tempo_change() {
        // 纯积分：前 480 ticks @120BPM (0.5s) + 后 480 ticks @240BPM (0.25s) = 0.75s
        // （直接测 ticks_to_seconds；duration_seconds 有 .max(1.0) 业务钳制不适用此例）
        let d = ticks_to_seconds(960, 480, &[(480, 240.0)]);
        assert!(
            (d - 0.75).abs() < 0.001,
            "tempo 变化积分应为 0.75s，实际 {d}"
        );
        // 包装层钳制生效：短时长被抬到 ≥1s
        assert!((duration_seconds(960, 480, &[(480, 240.0)]) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_ticks_to_seconds_zero_ppq_safe() {
        // ppq=0 时直接按 tick 数返回，不 panic
        assert!((ticks_to_seconds(100, 0, &[]) - 100.0).abs() < f64::EPSILON);
    }
}
