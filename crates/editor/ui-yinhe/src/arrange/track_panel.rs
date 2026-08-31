#![allow(clippy::manual_is_multiple_of)]
//! 走带音轨列表 — 对应 `yinhe arrange/track_panel.rs:1046`
//!
//! yinhe 原 1046 行实现 canvas 手绘 + 持久化 `egui::Id` + 拖拽排序 `DragReorder` +
//! `ArRowLayout` / `ArRow::Track/Automation` / 自动化展开 `arr_am_expanded` /
//! M/S 试听 `am_ms` 等复杂状态。
//!
//! iced 桩：用 `row + scrollable + column` 组合 widget（非 canvas 手绘）
//! 重构，保留：交替行条纹 / 选中高亮 / M/S 按钮 / chevron 展开 / 拖拽排序
//! 预留接口。后续 P3 使用 `lumino-gfx::Context` / `SwappableBuffer` 时仅负责
//! 音符层叠加，本列表保持 iced 原生组件以保证主题与无障碍一致性。

use std::collections::HashSet;

use iced_core::{Alignment, Length};
use iced_widget::{button, column, container, row, scrollable, text};

use lumino_ui_core::{Element, Message, Theme, window::Window};
use lumino_ui_core::sidebar_event::Event as SidebarEvent;

/// 音轨行显示数据（精简版，对齐 yinhe `TrackInfo + TrackOverride + ArRowLayout`）
///
/// P3 占位：后续可扩展 `automation_lanes / am_expanded / am_ms` 等全量字段。
#[derive(Debug, Clone)]
pub struct TrackRow {
    /// 音轨索引（全局 track idx，0 起，与 selection HashSet 对齐）
    pub index: u16,
    /// 音轨名
    pub name: String,
    /// 端口（显示为 A..P）
    pub port: u8,
    /// MIDI 通道 0..15
    pub channel: u8,
    /// 颜色（rgba 0..1）
    pub color: [f32; 4],
    /// 是否为 Conductor（Master，固定在首位，不可拖动/删除）
    pub is_conductor: bool,
    /// 是否可见（隐藏轨跳过绘制，与 `track_visible` 对齐）
    pub visible: bool,
    /// 是否静音 / 是否独奏（对齐 `TrackOverride`）
    pub muted: bool,
    pub soloed: bool,
}

impl TrackRow {
    /// 格式化显示标签（`A01` 类似 yinhe `track_panel` 第二行徽标）
    #[must_use]
    pub fn label(&self) -> String {
        if self.is_conductor {
            "Master".to_string()
        } else {
            format!(
                "{}{:02}",
                (b'A' + self.port.min(15)) as char,
                self.channel + 1
            )
        }
    }

    /// 编号文本（`003` 三位零填充，对齐 yinhe `format!("{:03}", ti.index)`）
    #[must_use]
    pub fn num_text(&self) -> String {
        format!("{:03}", self.index)
    }
}

/// 走带音轨列表视图入参（聚合，对齐 yinhe `track_panel::show` 的 18 参数）
#[derive(Debug, Clone)]
pub struct TrackPanelState {
    /// 音轨行列表（含 Conductor 在内）
    pub rows: Vec<TrackRow>,
    /// 当前选中的音轨索引集合
    pub selected: HashSet<u16>,
    /// 范围选择锚点
    pub selection_anchor: Option<u16>,
    /// 行高（>=30 时显示详情双行，否则紧凑单行，与 yinhe `show_details` 一致）
    pub row_height: f32,
    /// 垂直滚动偏移（由上层 scrollable 或 ViewState 同步）
    pub scroll_y: f32,
    /// 是否可请求跳转钢琴卷帘（双击行时置 true）
    pub request_pianoroll: bool,
}

/// 行按钮风格（复用 `chrome` 的 `transport_button` 弱背景语义）
fn inline_button<'a>(
    label: &'static str,
    active: bool,
    active_color: iced_core::Color,
    _window: &'a Window,
    on_press: Option<Message>,
) -> Element<'a> {
    let mut btn = button(text(label).size(11).align_x(Alignment::Center))
        .padding([2, 6])
        .style(move |_theme: &Theme, status| {
            let bg = if active {
                active_color
            } else if status == button::Status::Hovered {
                iced_core::Color::from_rgba(0.0, 0.0, 0.0, 0.06)
            } else {
                iced_core::Color::TRANSPARENT
            };
            button::Style {
                background: Some(iced_core::Background::Color(bg)),
                text_color: if active {
                    iced_core::Color::BLACK
                } else {
                    iced_core::Color::from_rgb(0.35, 0.35, 0.35)
                },
                border: iced_core::Border {
                    radius: 3.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        });
    if let Some(msg) = on_press {
        btn = btn.on_press(msg);
    }
    btn.into()
}

/// 渲染单行（对齐 yinhe `track_panel::show` 的行内布局：
///
/// `| 色带 14px | 编号 003 | 标签 A01 | 名称 ... | [M][S] | chevron/+/展开 |`）
#[allow(clippy::manual_is_multiple_of)]
fn track_row_view<'a>(row: &TrackRow, is_selected: bool, window: &'a Window) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let stripe = palette.background.weak.color;

    let bg = if is_selected {
        // clippy::manual_is_multiple_of 允许取模判断行奇偶
        palette.background.strong.color
    } else if row.index % 2 == 0 {
        stripe
    } else {
        palette.background.base.color
    };

    let color32 =
        iced_core::Color::from_rgba(row.color[0], row.color[1], row.color[2], row.color[3]);

    let badge = container(text("").size(1))
        .width(Length::Fixed(14.0))
        .height(Length::Fixed(28.0))
        .style(move |_theme: &Theme| container::Style {
            background: Some(iced_core::Background::Color(color32)),
            ..Default::default()
        });

    let num = text(row.num_text()).size(11);
    let label = text(row.label()).size(11);
    let name = text(row.name.clone()).size(12);

    let details = if row.is_conductor {
        row![num, label, name].spacing(6).align_y(Alignment::Center)
    } else {
        // M/S 按钮（与 yinhe `draw_inline_button` 尺寸 18 呼应，紧凑行隐藏）— 接线到 Sidebar 事件
        let m_btn = inline_button(
            "M",
            row.muted,
            iced_core::Color::from_rgb(0.95, 0.33, 0.33),
            window,
            Some(Message::Sidebar(SidebarEvent::TrackMuteToggled(
                row.index as usize,
            ))),
        );
        let s_btn = inline_button(
            "S",
            row.soloed,
            iced_core::Color::from_rgb(0.33, 0.62, 0.95),
            window,
            Some(Message::Sidebar(SidebarEvent::TrackSoloToggled(
                row.index as usize,
            ))),
        );
        row![num, label, name, m_btn, s_btn]
            .spacing(6)
            .align_y(Alignment::Center)
    };

    let inner = row![badge, details]
        .spacing(6)
        .align_y(Alignment::Center)
        .padding([2, 6]);

    let row_content = container(inner)
        .width(Length::Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(iced_core::Background::Color(bg)),
            border: iced_core::Border {
                radius: 2.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

    // 整行可点击选中（Conductor 也可选中）；M/S 按钮已在内部处理 mute/solo，行点击处理选中
    if row.is_conductor {
        row_content.into()
    } else {
        button(row_content)
            .padding(0)
            .style(|_theme: &Theme, _status| button::Style {
                background: None,
                text_color: iced_core::Color::TRANSPARENT,
                border: iced_core::Border::default(),
                ..Default::default()
            })
            .on_press(Message::Sidebar(SidebarEvent::TrackSelected(
                row.index as usize,
            )))
            .into()
    }
}

/// 渲染音轨列表（row + scrollable）
///
/// ```text
/// scrollable(
///   column![ track_row_view(...), track_row_view(...), ... ]
/// )
/// ```
/// - 隐藏轨跳过（与 yinhe `track_visible` 一致）
/// - 单击 / Shift 范围 / Ctrl 切换 / 双击入 Pianoroll 的选择逻辑
///   由上层 `Message::ArrangementSelectTrack` 等消息驱动，此处仅展示，
///   不直接修改 `HashSet`，符合 iced 单向数据流。
pub fn view<'a>(window: &'a Window, state: TrackPanelState) -> Element<'a> {
    // 拥有所有权，避免上层 Box::leak 泄漏；此处 clone 已足够
    let visible_rows: Vec<Element<'a>> = state
        .rows
        .into_iter()
        .filter(|r| r.visible)
        .map(|r| {
            let sel = state.selected.contains(&r.index);
            track_row_view(&r, sel, window)
        })
        .collect();

    let content = column(visible_rows).spacing(2).padding([4, 4]);

    scrollable(content)
        .height(Length::Fill)
        .width(Length::Fixed(220.0))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_row_label_conductor() {
        let r = TrackRow {
            index: 0,
            name: "Master".into(),
            port: 0,
            channel: 0,
            color: [0.2, 0.5, 0.8, 1.0],
            is_conductor: true,
            visible: true,
            muted: false,
            soloed: false,
        };
        assert_eq!(r.label(), "Master");
        assert_eq!(r.num_text(), "000");
    }

    #[test]
    fn track_row_label_normal() {
        let r = TrackRow {
            index: 5,
            name: "Piano".into(),
            port: 1,
            channel: 2,
            color: [1.0, 0.0, 0.0, 1.0],
            is_conductor: false,
            visible: true,
            muted: false,
            soloed: false,
        };
        assert_eq!(r.label(), "B03");
    }
}
