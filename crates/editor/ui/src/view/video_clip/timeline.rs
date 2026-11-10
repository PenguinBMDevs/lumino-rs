//! 视频剪辑剪辑带时间轴（组装层）
//!
//! 结构对标钢琴卷帘：内容区（[`TimelineCanvas`] 自绘）+ 底部
//! [`ScrollbarWidget`] 滚动条行（滑块拖拽滚动、边缘拖拽缩放，卷帘同款样式）。

use iced_core::Length;
use iced_widget::{column, container};

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

/// 秒 → tick（[`ticks_to_seconds`] 的逆运算，共用同一 tempo 分段积分）。
///
/// 画面进出显示用：剪辑面板是**秒域**时钟，而瀑布流/卷帘渲染锚点是
/// **tick 域**，两者必须经本函数换算才能保证「画面帧 == 素材带当前时刻」。
/// 逐段累加秒数定位所在 tempo 段，再在该段 bpm 下线性换算（与正向积分互逆）。
pub(crate) fn seconds_to_ticks(secs: f64, ppq: u32, tempos: &[(u32, f32)]) -> u64 {
    let secs = if secs.is_finite() { secs.max(0.0) } else { 0.0 };
    if ppq == 0 {
        return secs as u64;
    }
    let mut prev_tick: u32 = 0;
    let mut prev_bpm: f32 = 120.0;
    let mut acc_secs: f64 = 0.0;
    for &(t, bpm) in tempos {
        let seg_ticks = t.saturating_sub(prev_tick) as f64;
        let seg_secs = seg_ticks * 60.0 / (prev_bpm as f64 * ppq as f64);
        if secs <= acc_secs + seg_secs {
            let within = secs - acc_secs;
            let ticks = prev_tick as f64 + within * prev_bpm as f64 * ppq as f64 / 60.0;
            return ticks.max(0.0) as u64;
        }
        acc_secs += seg_secs;
        prev_tick = t;
        prev_bpm = bpm;
    }
    let ticks = prev_tick as f64 + (secs - acc_secs) * prev_bpm as f64 * ppq as f64 / 60.0;
    ticks.max(0.0) as u64
}

/// 渲染剪辑带时间轴的**占位骨架**。
///
/// 内容（标题行 / 传输按钮 / 标尺与双轨 / 播放头 / 水平滚动条）已全部由 egui
/// 绘制并接管输入；iced 侧只保留同尺寸结构与三个探针，为 egui 提供实测矩形：
/// - `renderer::timeline_title`（标题行）
/// - `renderer::clip_canvas`（标尺 + 双轨画布）
/// - `renderer::timeline_scrollbar`（水平滚动条）
pub fn timeline_pane(theme: &Theme) -> crate::Element<'static> {
    let palette = theme.extended_palette();
    let weak_color = palette.background.weak.color;
    let strong_color = palette.background.strong.color;

    let title_row = crate::view::video_clip::rect_probe::probe(
        "renderer::timeline_title",
        iced_widget::container(
            iced_widget::Space::new()
                .width(Length::Fill)
                .height(Length::Fixed(20.0)),
        )
        .width(Length::Fill)
        .padding([4, 8]),
    );
    let canvas = crate::view::video_clip::rect_probe::probe(
        "renderer::clip_canvas",
        iced_widget::container(iced_widget::Space::new())
            .width(Length::Fill)
            .height(Length::Fill),
    );
    let scrollbar = crate::view::video_clip::rect_probe::probe(
        "renderer::timeline_scrollbar",
        iced_widget::container(
            iced_widget::Space::new()
                .width(Length::Fill)
                .height(Length::Fixed(20.0)),
        )
        .width(Length::Fill),
    );

    container(
        column![title_row, canvas, scrollbar]
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

    /// 画面进出显示的地基：秒域时钟 → tick 域渲染锚点必须与正向换算互逆。
    #[test]
    fn test_seconds_to_ticks_inverse_of_ticks_to_seconds() {
        // 空 tempo 表：120BPM / ppq=480 → 960 tick = 1s
        assert_eq!(seconds_to_ticks(1.0, 480, &[]), 960);
        assert_eq!(seconds_to_ticks(0.0, 480, &[]), 0);
        // 负值 / NaN 兜底到 0，不得 panic、不得产生巨值
        assert_eq!(seconds_to_ticks(-3.0, 480, &[]), 0);
        assert_eq!(seconds_to_ticks(f64::NAN, 480, &[]), 0);
        // ppq=0 时按秒当 tick（与 ticks_to_seconds 同款退化分支）
        assert_eq!(seconds_to_ticks(100.0, 0, &[]), 100);

        // tempo 变化：前 480 tick @120BPM（0.5s），其后 @240BPM
        let tempos = [(480, 240.0)];
        assert_eq!(seconds_to_ticks(0.5, 480, &tempos), 480);
        assert_eq!(seconds_to_ticks(0.75, 480, &tempos), 960);

        // 互逆：任意 tick 正反两次换算回到原值（容差 1 tick）
        for tick in [0u64, 100, 480, 960, 1200, 5000] {
            let secs = ticks_to_seconds(tick, 480, &tempos);
            let back = seconds_to_ticks(secs, 480, &tempos);
            assert!(
                (back as i64 - tick as i64).abs() <= 1,
                "tick {tick} → {secs}s → {back} 不互逆"
            );
        }
    }
}
