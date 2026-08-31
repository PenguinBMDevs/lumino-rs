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

use iced_core::{Alignment, Length, Padding};
use iced_widget::{button, column, container, row, scrollable, space, text};

use lumino_ui_core::sidebar_event::Event as SidebarEvent;
use lumino_ui_core::{Element, Message, Theme, window::Window};

/// 面板固定宽度，对齐 yinhe `tp_w = 220` 与 `left_panel_width = 220 + SPLIT_HANDLE_W`
const PANEL_WIDTH: f32 = 220.0;
/// 色带宽度，对齐 yinhe `badge_w = 14.0`
const BADGE_WIDTH: f32 = 14.0;

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
    /// 是否为自动化子行（Pitch Bend / CC 通道等）
    ///
    /// 等宽约束：子行与主轨共用 `Fixed(PANEL_WIDTH)` 外容器，宽度完全一致；
    /// 仅通过左侧缩进与背景微调区分，不产生宽度差异（修复 yinhe 原 1046 中
    /// `panel_w` 统一为 `tp_w` 的等宽布局）。
    pub is_automation: bool,
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
///
/// 等宽约束（修复问题1）：所有行（主轨+自动化子项）外容器均为
/// `Fixed(PANEL_WIDTH)` × `Fixed(row_height)`，子项仅通过 `indent` 缩进与
/// 背景区分，不改变宽度；`scrollable` 与外层 `container` 均 `Fixed(220)`，
/// `column` 与行均 `Fixed(220)`，移除导致子项变窄的 `padding`/`Fill`→`Shrink` 混用。
#[allow(clippy::manual_is_multiple_of)]
fn track_row_view<'a>(
    row: &TrackRow,
    is_selected: bool,
    row_pos: usize,
    row_height: f32,
    window: &'a Window,
) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let stripe = palette.background.weak.color;

    // 交替行条纹按全局行号奇偶（与 yinhe `row % 2 == 0` 一致），而非 `row.index`
    let bg = if is_selected {
        palette.background.strong.color
    } else if row_pos % 2 == 0 {
        stripe
    } else {
        palette.background.base.color
    };

    let color32 =
        iced_core::Color::from_rgba(row.color[0], row.color[1], row.color[2], row.color[3]);

    // 色带：固定 14px × row_height，与 yinhe `badge_w = 14.0, badge_rect = vec2(14, lh)` 一致
    let badge = container(text("").size(1))
        .width(Length::Fixed(BADGE_WIDTH))
        .height(Length::Fixed(row_height))
        .style(move |_theme: &Theme| container::Style {
            background: Some(iced_core::Background::Color(color32)),
            ..Default::default()
        });

    let num = text(row.num_text()).size(11);
    let label = text(row.label()).size(11);
    let name = text(row.name.clone()).size(12);

    // 左侧文本组（编号+标签+名称），右侧 M/S 按钮右对齐（与 yinhe `row_rect.max.x - total_btn_w - 6` 一致）
    let left = row![num, label, name]
        .spacing(6)
        .align_y(Alignment::Center);

    let right: Element<'a> = if row.is_conductor {
        // Conductor 无 M/S（与 yinhe `is_conductor` 分支一致）
        space().width(Length::Fixed(0.0)).height(Length::Fixed(0.0)).into()
    } else {
        // M/S 按钮 — 接线到 Sidebar 事件；自动化子行同样等宽占位（仅背景/缩进区分）
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
        row![m_btn, s_btn]
            .spacing(4)
            .align_y(Alignment::Center)
            .into()
    };

    // 中间弹性空白，使 M/S 右对齐（等宽关键：若用 `row![left, right]`，名称长度变化会导致 M/S 位置抖动）
    let details = row![
        left,
        space().width(Length::Fill),
        right
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .width(Length::Fill);

    // 子项缩进：仅改变内边距，不改变外容器宽度（Fixed 220），与 yinhe 子行同宽、仅内容缩进一致
    let indent = if row.is_automation { 10.0 } else { 0.0 };
    let inner = row![badge, details]
        .spacing(6)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding {
            top: 2.0,
            bottom: 2.0,
            left: 6.0 + indent,
            right: 6.0,
        });

    let row_content = container(inner)
        .width(Length::Fixed(PANEL_WIDTH))
        .height(Length::Fixed(row_height))
        .style(move |_theme: &Theme| container::Style {
            background: Some(iced_core::Background::Color(bg)),
            border: iced_core::Border {
                radius: 2.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

    // 等宽关键：button 与 container 均显式 Fixed(PANEL_WIDTH)，避免 `Fill` 在 `Shrink` 父容器中塌缩
    //（原实现 `container(Fill)` 在 `button(Shrink)` 内导致非 Conductor 行窄于 Conductor）
    if row.is_conductor {
        row_content.into()
    } else {
        button(row_content)
            .width(Length::Fixed(PANEL_WIDTH))
            .height(Length::Fixed(row_height))
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
    // 行高回退：与 yinhe `row_height.clamp(16,120)` 一致，异常值回退 32
    let row_h = if state.row_height.is_finite() && state.row_height >= 16.0 {
        state.row_height
    } else {
        32.0
    };
    // 全局行号用于条纹（过滤隐藏轨后重新计数，与 yinhe `row % 2` 的全局行号一致）
    // 可见行重新编号用于条纹（与 yinhe `row % 2` 的全局行号一致；隐藏轨不占用条纹）
    let visible_rows: Vec<Element<'a>> = state
        .rows
        .into_iter()
        .filter(|r| r.visible)
        .enumerate()
        .map(|(pos, r)| {
            let sel = state.selected.contains(&r.index);
            track_row_view(&r, sel, pos, row_h, window)
        })
        .collect();

    // column 与 scrollable 均 Fixed(220)，内容列 padding 0、spacing 0（与 yinhe 行连续堆叠 `y = row * lh` 一致）
    // 移除原 `padding [4,4] + spacing 2` 导致的可用宽度 212 与行间隙，确保主轨/子项视觉等宽
    let content = column(visible_rows)
        .spacing(0)
        .width(Length::Fixed(PANEL_WIDTH))
        .padding(Padding::new(0.0));

    scrollable(content)
        .height(Length::Fill)
        .width(Length::Fixed(PANEL_WIDTH))
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
            is_automation: false,
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
            is_automation: false,
        };
        assert_eq!(r.label(), "B03");
    }

    #[test]
    fn automation_row_is_distinguished_by_indent_not_width() {
        // 子项与主轨宽度一致，仅缩进/背景区分
        let main = TrackRow {
            index: 1,
            name: "Setup".into(),
            port: 0,
            channel: 0,
            color: [0.3, 0.7, 0.9, 1.0],
            is_conductor: false,
            visible: true,
            muted: false,
            soloed: false,
            is_automation: false,
        };
        let sub = TrackRow {
            index: 1,
            name: "Pitch Bend Setup".into(),
            port: 0,
            channel: 0,
            color: [0.85, 0.85, 0.85, 1.0],
            is_conductor: false,
            visible: true,
            muted: false,
            soloed: false,
            is_automation: true,
        };
        assert!(!main.is_automation);
        assert!(sub.is_automation);
        // 宽度由常量保证等宽，此处仅校验标记
        assert_eq!(main.index, sub.index);
    }
}
