//! 检测仪表盘（已移至工具选择区框左侧）
//!
//! 移植自 yinhe 项目 `chrome/transport_bar.rs` 的 `show_timecode_display`：
//! 顶部数据显示模块（CPU 占用、内存占用、播放时间等）。
//!
//! 说明：yinhe 使用 egui 渲染该模块，而 Lumino 使用 iced，无法逐字照抄 egui 代码。
//! 此处按 yinhe 的**设计语言**（三列 + 上下双行文字 + 强调色 + 垂直分隔线）
//! 用 iced 重写，背景/文字色全部通过主题 palette 自动适配，与工具栏工具框风格统一：
//! - 背景色：`palette.background.weak`（与工具选择框一致）
//! - 数值色：`palette.primary.strong`（主题强调色）
//! - 标签色：`palette.background.weak.text`（标签说明文字）
//! - 分隔线：`palette.background.strong`（微高于背景色，保证可见）
//! - CPU / 内存：来自 `PerfData`（由 `CpuMonitor` + `lumino_memory_monitor` 计算）
//! - 播放时间 / BPM：由 `tempo_points` + `ppq` 将播放位置（tick）换算得到

use iced_core::Alignment;
use iced_widget::{button, column, container, row, space, text, text_input};

use crate::statusbar::performance::PerfData;
use crate::toolbar::{Event, Toolbar};
use crate::{Element, Theme};
use lumino_core::midi_types::TempoPoint;

impl Toolbar {
    /// 渲染检测仪表盘（yinhe 同款时间码显示）
    ///
    /// 三列布局（对齐 yinhe `show_timecode_display`）：
    /// - 列 0：CPU 占用（上） / 内存占用（下）
    /// - 列 1：当前 BPM（上） / PPQ（下）
    /// - 列 2：播放位置 小节.拍（上） / 播放时间 mm:ss.cs（下）
    pub fn render_detection_dashboard<'a>(
        &'a self,
        content_height: f32,
        palette: &'a iced_core::theme::palette::Extended,
        perf_data: &PerfData,
        playback_tick: f32,
        ppq: u16,
        tempo_points: &[TempoPoint],
    ) -> Element<'a> {
        let cpu = perf_data.cpu_usage;
        let mem = perf_data.memory_mb;
        let bpm = bpm_at_tick(tempo_points, playback_tick);
        let time_secs = tick_to_seconds(tempo_points, ppq, playback_tick);
        let pos = format_position(playback_tick, ppq);

        let accent = palette.primary.strong.color;
        let dim = palette.background.weak.text;
        let sep_color = palette.background.strong.color;
        let sep_h = (content_height * 0.55).max(8.0);

        let c0 = metric_column_with_button(
            format!("{cpu:.1}%"),
            format!("{mem:.1} MB"),
            accent,
            dim,
            76.0,
        );
        let c1 = if self.ppq_editing {
            ppq_edit_column(
                format!("{bpm:.1}"),
                &self.ppq_edit_buffer,
                accent,
                dim,
                80.0,
            )
        } else {
            ppq_display_column(format!("{bpm:.1}"), ppq, accent, dim, 80.0)
        };
        let c2 = metric_column(pos, format_time(time_secs), accent, dim, 86.0);

        container(
            row![
                c0,
                v_separator(sep_h, sep_color),
                space().width(10),
                c1,
                v_separator(sep_h, sep_color),
                space().width(10),
                c2,
            ]
            .align_y(Alignment::Center),
        )
        .height(content_height)
        .padding([0, 12])
        .align_y(iced_core::alignment::Vertical::Center)
        .style(move |_t: &Theme| {
            container::Style::default()
                .background(palette.background.weak.color)
                .border(iced_core::Border {
                    radius: 4.0.into(),
                    width: 0.0,
                    color: iced_core::Color::TRANSPARENT,
                })
        })
        .into()
    }
}

/// 单列（上行强调色数值 + 下行弱化说明文字），固定宽度居中显示
fn metric_column<'a>(
    top: String,
    bot: String,
    accent: iced_core::Color,
    dim: iced_core::Color,
    width: f32,
) -> Element<'a> {
    column![
        text(top).size(13).style(move |_t: &Theme| text::Style {
            color: Some(accent),
        }),
        space().height(2),
        text(bot)
            .size(11)
            .style(move |_t: &Theme| text::Style { color: Some(dim) }),
    ]
    .width(iced_widget::core::Length::Fixed(width))
    .align_x(Alignment::Center)
    .into()
}

/// 带按钮的单列：上行强调色数值 + 下行可点击的弱化说明文字
fn metric_column_with_button<'a>(
    top: String,
    bot: String,
    accent: iced_core::Color,
    dim: iced_core::Color,
    width: f32,
) -> Element<'a> {
    let memory_button = button(text(bot).size(11))
        .on_press(crate::toolbar::Event::open_memory_monitor_dialog())
        .padding([0.0, 0.0])
        .style(move |_t: &Theme, _status| button::Style {
            background: None,
            text_color: dim,
            border: iced_core::Border::default(),
            shadow: Default::default(),
            snap: false,
        });

    column![
        text(top).size(13).style(move |_t: &Theme| text::Style {
            color: Some(accent),
        }),
        space().height(2),
        memory_button,
    ]
    .width(iced_widget::core::Length::Fixed(width))
    .align_x(Alignment::Center)
    .into()
}

/// 垂直分隔线（1px 宽）
fn v_separator<'a>(height: f32, color: iced_core::Color) -> Element<'a> {
    container(space())
        .width(1)
        .height(height)
        .style(move |_t: &Theme| container::Style::default().background(color))
        .into()
}

/// 可点击的 PPQ 显示列：点击后进入编辑模式
fn ppq_display_column<'a>(
    top: String,
    ppq: u16,
    accent: iced_core::Color,
    dim: iced_core::Color,
    width: f32,
) -> Element<'a> {
    let ppq_button = button(
        text(format!("PPQ {ppq}"))
            .size(11)
            .style(move |_t: &Theme| text::Style { color: Some(dim) }),
    )
    .on_press(Event::ppq_edit_toggled(ppq))
    .padding([0.0, 0.0])
    .style(move |_t: &Theme, _status| button::Style {
        background: None,
        text_color: dim,
        border: iced_core::Border::default(),
        shadow: Default::default(),
        snap: false,
    });

    column![
        text(top).size(13).style(move |_t: &Theme| text::Style {
            color: Some(accent),
        }),
        space().height(2),
        ppq_button,
    ]
    .width(iced_widget::core::Length::Fixed(width))
    .align_x(Alignment::Center)
    .into()
}

/// PPQ 编辑列：显示 TextInput 供用户编辑 PPQ 值
fn ppq_edit_column<'a>(
    top: String,
    edit_buffer: &'a str,
    accent: iced_core::Color,
    _dim: iced_core::Color,
    width: f32,
) -> Element<'a> {
    let input_style = move |_t: &iced_core::Theme| container::Style {
        background: Some(iced_core::Background::Color(iced_core::Color::from_rgba(
            0.0, 0.0, 0.0, 0.0,
        ))),
        border: iced_core::Border {
            radius: 0.0.into(),
            width: 0.0,
            color: iced_core::Color::TRANSPARENT,
        },
        ..Default::default()
    };

    column![
        text(top).size(13).style(move |_t: &Theme| text::Style {
            color: Some(accent),
        }),
        space().height(2),
        container(
            text_input("PPQ", edit_buffer)
                .size(11)
                .on_input(Event::ppq_edit_changed)
                .on_submit(Event::ppq_edit_confirmed())
                .padding([0, 4])
                .width(iced_widget::core::Length::Fixed(width - 8.0))
        )
        .width(iced_widget::core::Length::Fixed(width - 4.0))
        .style(input_style),
    ]
    .width(iced_widget::core::Length::Fixed(width))
    .align_x(Alignment::Center)
    .into()
}

/// 取播放位置处的当前 BPM（取 tick 之前最近的速度点）
fn bpm_at_tick(points: &[TempoPoint], tick: f32) -> f64 {
    let mut bpm = points.first().map(|p| p.bpm).unwrap_or(120.0);
    for p in points {
        if p.tick <= tick {
            bpm = p.bpm;
        } else {
            break;
        }
    }
    bpm
}

/// 将 tick 按 tempo 变化表换算为秒（与 yinhe `tick_to_seconds` 等价）
fn tick_to_seconds(points: &[TempoPoint], ppq: u16, tick: f32) -> f64 {
    let ppq = ppq as f64;
    if points.is_empty() {
        return (tick as f64 / ppq) / 120.0 * 60.0;
    }
    let mut seconds = 0.0_f64;
    for i in 0..points.len() {
        let seg_start = points[i].tick as f64;
        if seg_start > tick as f64 {
            break;
        }
        let seg_bpm = points[i].bpm;
        let seg_end = if i + 1 < points.len() {
            points[i + 1].tick as f64
        } else {
            tick as f64
        };
        let consume = (seg_end.min(tick as f64) - seg_start).max(0.0);
        seconds += consume / ppq / seg_bpm * 60.0;
    }
    seconds
}

/// 格式化时间为 mm:ss.cs（百分秒）
fn format_time(secs: f64) -> String {
    let total_cs = (secs * 100.0).max(0.0) as u64;
    let cs = total_cs % 100;
    let total_s = total_cs / 100;
    let s = total_s % 60;
    let m = total_s / 60;
    format!("{m:02}:{s:02}.{cs:02}")
}

/// 格式化播放位置为 小节.拍（假设 4/4 拍，Lumino 未追踪拍号事件）
fn format_position(tick: f32, ppq: u16) -> String {
    let ppq = ppq as f32;
    let ticks_per_bar = ppq * 4.0;
    if ticks_per_bar <= 0.0 {
        return "0.0".to_string();
    }
    let bar = (tick / ticks_per_bar).floor().max(0.0) as u32 + 1;
    let beat = (((tick % ticks_per_bar) / ppq).floor()).max(0.0) as u32 + 1;
    format!("{bar}.{beat}")
}
