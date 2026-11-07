//! 状态栏指标渲染：`metric_*` 文本工厂 + GPU (LGS) 指标组。
//!
//! 从 `statusbar.rs` 拆出（该文件曾超过 400 行上限）；渲染语义与拆分前逐字一致。

use iced_widget::{button, row, space, text};
use lumino_core::storage::config::GpuLoadDisplay;
use lumino_midi_io::GpuBufferStats;

use super::{GpuStatusFlags, QueueTrend};
use crate::Element;
use crate::Theme;

/// 指标颜色档位。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetricTone {
    /// 常规（强调色）
    Normal,
    /// 警告（黄）
    Warning,
    /// 危险（红）
    Danger,
}

/// 负载分级：<60% 常规；60–90% 警告；≥90% 危险（1.0 = 90% 实时预算上限）。
pub(crate) fn load_tone(load: f64) -> MetricTone {
    if load >= 0.9 {
        MetricTone::Danger
    } else if load >= 0.6 {
        MetricTone::Warning
    } else {
        MetricTone::Normal
    }
}

/// 欠载分级：0 = 常规；>0 = 危险（欠载即已静音丢音）。
pub(crate) fn underrun_tone(underruns: u64) -> MetricTone {
    if underruns == 0 {
        MetricTone::Normal
    } else {
        MetricTone::Danger
    }
}

/// 治理级别分级：0=常规 / 1=警告 / ≥2=危险（2=Overload / 3=Emergency）。
pub(crate) fn governor_tone(level: u64) -> MetricTone {
    match level {
        0 => MetricTone::Normal,
        1 => MetricTone::Warning,
        _ => MetricTone::Danger,
    }
}

/// 构建 GPU 指标组（Q 队列毫秒 + 趋势箭头 / LOAD / V / UR /
/// 采样内存 GMEM/CMEM，异常时附 GOV、DROP）。
pub(super) fn build_gpu_section<'a>(
    stats: GpuBufferStats,
    trend: QueueTrend,
    flags: GpuStatusFlags,
) -> Element<'a> {
    let load = match flags.load_mode {
        GpuLoadDisplay::ShortAverage => stats.average_render_load,
        GpuLoadDisplay::Instant => stats.render_load,
    };

    // Q 毫秒整数：量程 0–600ms（容量按时间封顶），毫秒分辨率才能区分
    // 「缓冲还稳」与「下一块就断」；数据源本身即块级/回调级粒度（个位数 ms）。
    let queue_metric = format!("Q {:.0}ms{}", stats.queue_ms, trend.arrow());

    let mut row = row![
        metric_label("GPU"),
        metric_value(queue_metric),
        space().width(8),
        metric_label("LOAD"),
        metric_value_toned(format!("{:.0}%", load * 100.0), load_tone(load)),
        space().width(8),
        metric_label("V"),
        metric_value(stats.voice_count.to_string()),
        space().width(8),
        metric_label("UR"),
        metric_value_toned(stats.underruns.to_string(), underrun_tone(stats.underruns)),
    ]
    .spacing(6)
    .align_y(iced_core::Alignment::Center);

    // REF-LGS-013：采样内存（GPU / CPU）。未加载音源时为 0，不占位。
    if stats.sample_gpu_bytes > 0 || stats.sample_cpu_bytes > 0 {
        row = row.push(space().width(8));
        row = row.push(metric_label("GMEM"));
        row = row.push(metric_value(mem_mb(stats.sample_gpu_bytes)));
        row = row.push(space().width(8));
        row = row.push(metric_label("CMEM"));
        row = row.push(metric_value(mem_mb(stats.sample_cpu_bytes)));
    }

    // 防爆治理指标：仅在异常（级别 >0 或发生过丢弃）时出现，
    // 且受「显示防爆治理指标」副开关控制。
    if flags.show_governor {
        if stats.governor_level > 0 {
            row = row.push(space().width(8));
            row = row.push(metric_label("GOV"));
            row = row.push(metric_value_toned(
                stats.governor_level.to_string(),
                governor_tone(stats.governor_level),
            ));
        }
        if stats.dropped_note_ons > 0 {
            row = row.push(space().width(8));
            row = row.push(metric_label("DROP"));
            row = row.push(metric_value_toned(
                stats.dropped_note_ons.to_string(),
                MetricTone::Danger,
            ));
        }
    }

    row.into()
}

/// 字节 → MB 文本（1 位小数；0 显示 "0"）。REF-LGS-013 状态栏用。
fn mem_mb(bytes: u64) -> String {
    if bytes == 0 {
        "0".to_string()
    } else {
        format!("{:.1}M", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// 指标标签（弱色小字，仿照 yinhe mode_bar 的 `metric` 标签）
pub(super) fn metric_label<'a>(label: &'a str) -> Element<'a> {
    text(label)
        .size(12)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            text::Style {
                color: Some(palette.background.weak.text),
            }
        })
        .into()
}

/// 指标数值（强调色，仿照 yinhe mode_bar 的 `metric` 数值）
pub(super) fn metric_value<'a>(value: String) -> Element<'a> {
    text(value)
        .size(12)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            text::Style {
                color: Some(palette.primary.strong.color),
            }
        })
        .into()
}

/// 按颜色档位渲染的指标数值（常规=强调色 / 警告=黄 / 危险=红）。
fn metric_value_toned<'a>(value: String, tone: MetricTone) -> Element<'a> {
    text(value)
        .size(12)
        .style(move |theme: &Theme| {
            let palette = theme.extended_palette();
            let color = match tone {
                MetricTone::Normal => palette.primary.strong.color,
                MetricTone::Warning => palette.warning.base.color,
                MetricTone::Danger => palette.danger.strong.color,
            };
            text::Style { color: Some(color) }
        })
        .into()
}

/// 可点击的指标数值（强调色，点击发送消息，仿照 yinhe `metric_clickable`）
pub(super) fn metric_clickable_value<'a>(value: String, on_press: crate::Message) -> Element<'a> {
    button(text(value).size(12).style(|theme: &Theme| {
        let palette = theme.extended_palette();
        text::Style {
            color: Some(palette.primary.strong.color),
        }
    }))
    .on_press(on_press)
    .padding([0.0, 0.0])
    .style(|theme: &Theme, _status| {
        let palette = theme.extended_palette();
        button::Style {
            background: None,
            text_color: palette.primary.strong.color,
            border: iced_core::Border::default(),
            shadow: Default::default(),
            snap: false,
        }
    })
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// REF-LGS-013：采样内存显示为 MB 1 位小数；0 不显示单位。
    #[test]
    fn test_mem_mb_format() {
        assert_eq!(mem_mb(0), "0");
        assert_eq!(mem_mb(1024 * 1024), "1.0M");
        assert_eq!(mem_mb(3 * 1024 * 1024 / 2), "1.5M");
        assert_eq!(mem_mb(17 * 1024 * 1024 + 1024 * 1024 / 10), "17.1M");
    }

    #[test]
    fn test_tone_thresholds() {
        assert_eq!(load_tone(0.0), MetricTone::Normal);
        assert_eq!(load_tone(0.59), MetricTone::Normal);
        assert_eq!(load_tone(0.6), MetricTone::Warning);
        assert_eq!(load_tone(0.89), MetricTone::Warning);
        assert_eq!(load_tone(0.9), MetricTone::Danger);
        assert_eq!(load_tone(2.5), MetricTone::Danger, ">100% 仍为危险档");

        assert_eq!(underrun_tone(0), MetricTone::Normal);
        assert_eq!(underrun_tone(3), MetricTone::Danger);
        assert_eq!(governor_tone(0), MetricTone::Normal);
        assert_eq!(governor_tone(1), MetricTone::Warning);
        assert_eq!(governor_tone(2), MetricTone::Danger);
        assert_eq!(governor_tone(3), MetricTone::Danger);
    }
}
