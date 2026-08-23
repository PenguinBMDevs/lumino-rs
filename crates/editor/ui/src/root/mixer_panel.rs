//! 浮动混音台面板（覆盖层，非阻塞）+ 入口按钮
//!
//! 面板以 `Stack` 顶层叠加于钢琴卷帘主视图之上：外层全屏容器不持有任何事件
//! 处理器，仅面板内部的按钮响应事件，因此点击面板外部会穿透到下方钢琴卷帘
//! （非阻塞覆盖层，符合 yinhe 风格的可浮动混音台）。
//!
//! 增益/声像经 `Event::TrackGainChanged` / `TrackPanChanged` 下发，最终在
//! `Root` 侧同步到播放引擎（XSynth 合成管线末端的音频域增益/声像）。
//! 静音/独奏复用 `TrackMuteToggled` / `TrackSoloToggled`（单一来源）。
//!
//! 拖拽：iced 0.14 的 `mouse_area` 无 `on_drag`，改用标题栏 `on_press`
//! （开始）/ `on_move`（相对坐标）/ `on_release`（结束）组合；以 `offset +=
//! (p - grab)` 递推跟随光标（单次事件延迟，无感知）。拖拽边界夹在左侧栏
//! （48px）之外、屏幕范围内。

use crate::root::Root;
use crate::{Element, Theme, resources::icon, sidebar::Track};
use iced_core::{Background, Color, Length, Padding, alignment};
use iced_widget::{
    Space, button, column, container, mouse_area, row, scrollable, slider, text, vertical_slider,
};
use lumino_ui_core::sidebar_event::Event;

/// 浮动混音台面板状态
#[derive(Debug, Clone, Default)]
pub(crate) struct MixerPanelState {
    /// 面板是否打开
    pub open: bool,
    /// 面板主体是否展开（最大化）；false = 仅显示标题栏（最小化）
    pub maximized: bool,
    /// 面板左上角相对主内容区左下角的偏移（拖拽累加，逻辑像素）。
    /// `offset.0` = 距左边界内缩（= 面板左缘 x），`offset.1` = 距底边界内缩。
    pub offset: (f32, f32),
    /// 是否正在拖拽（标题栏按下且未松开）
    pub(crate) dragging: bool,
    /// 拖拽抓取点（首次 on_move 时记录的标题栏相对坐标），用于计算移动增量
    pub(crate) grab: Option<(f32, f32)>,
}

/// 入口按钮：钢琴卷帘小组底部左侧悬浮，点亮（MixerActive）表示面板打开。
pub(crate) fn view_mixer_entry(root: &Root) -> Element<'static> {
    let icon_enum = if root.mixer_panel.open {
        icon::Icon::MixerActive
    } else {
        icon::Icon::Mixer
    };
    let icon_img = icon::view_with_size_and_theme(icon_enum, 22, 22, Some(&root.window.theme));
    let btn = button(icon_img)
        .padding(8)
        .style(transparent_button)
        .on_press(Event::mixer_panel_toggled());
    container(btn)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(alignment::Horizontal::Left)
        .align_y(alignment::Vertical::Bottom)
        .padding(Padding {
            top: 0.0,
            right: 0.0,
            bottom: 8.0,
            left: 56.0,
        })
        .into()
}

/// 浮动混音台面板（关闭时返回 None）
pub(crate) fn view_mixer_panel(root: &Root) -> Option<Element<'static>> {
    if !root.mixer_panel.open {
        return None;
    }

    let header = build_header();
    let body = if root.mixer_panel.maximized {
        build_body(root)
    } else {
        column![].into()
    };

    let panel = container(column![header, body].spacing(2).width(Length::Shrink))
        .style(panel_background)
        .width(Length::Shrink)
        .height(Length::Shrink)
        .max_width(900);

    // 外层全屏容器：无事件处理器 → 点击外部穿透（非阻塞覆盖层）。
    // 面板以左下为锚点，offset 为距左/底边界的内缩 padding。
    Some(
        container(panel)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(alignment::Horizontal::Left)
            .align_y(alignment::Vertical::Bottom)
            .padding(Padding {
                top: 0.0,
                right: 0.0,
                bottom: root.mixer_panel.offset.1,
                left: root.mixer_panel.offset.0,
            })
            .into(),
    )
}

/// 标题栏：含最小化 / 关闭按钮；标题+中间空白为拖拽手柄（按钮区不触发拖拽）。
fn build_header() -> Element<'static> {
    let title = text("混音台").size(13);
    let minimize_btn = button(text("—").size(12))
        .padding(2)
        .style(transparent_button)
        .on_press(Event::mixer_panel_maximize_toggled());
    let close_btn = button(text("✕").size(12))
        .padding(2)
        .style(transparent_button)
        .on_press(Event::mixer_panel_toggled());
    let controls = row![minimize_btn, close_btn].spacing(4);

    // 拖拽手柄：标题 + 中间空白区域；按钮区独立，避免点击关闭/最小化误触拖拽。
    let drag_handle = mouse_area(
        row![title, Space::new().width(Length::Fill)]
            .spacing(8)
            .align_y(iced_core::Alignment::Center),
    )
    .on_press(Event::mixer_panel_drag_started())
    .on_move(|p| Event::mixer_panel_dragged(p.x, p.y))
    .on_release(Event::mixer_panel_drag_ended());

    let header_row = row![drag_handle, controls]
        .spacing(8)
        .align_y(iced_core::Alignment::Center);

    container(header_row)
        .padding(8)
        .width(Length::Fill)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(
                theme.extended_palette().background.strong.color,
            )),
            ..Default::default()
        })
        .into()
}

/// 面板主体：横向滚动的通道条列表。
fn build_body(root: &Root) -> Element<'static> {
    if root.sidebar.tracks.is_empty() {
        return container(text("暂无音轨").size(12)).padding(16).into();
    }

    let strips: Vec<Element<'static>> = root
        .sidebar
        .tracks
        .iter()
        .map(|track| build_strip(root, track))
        .collect();

    scrollable(row(strips).spacing(8).padding(8))
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new().width(8),
        ))
        .into()
}

/// 单条音轨的通道条：名称 + M/S + 增益推子（纵向）+ 声像（横向）。
fn build_strip(root: &Root, track: &Track) -> Element<'static> {
    // 提取为拥有值，避免闭包捕获 `&Track` 引用导致生命周期受限。
    let id = track.id;
    let is_muted = track.is_muted;
    let is_soloed = track.is_soloed;
    let display_label = track.display_label.clone();
    let strip = root.sidebar.mixer_strip(id);
    let name = text(display_label).size(11);
    let mute_btn = button(text("M").size(11))
        .padding(2)
        .style(move |theme: &Theme, _status| channel_button_style(theme, is_muted))
        .on_press(Event::track_mute_toggled(id));
    let solo_btn = button(text("S").size(11))
        .padding(2)
        .style(move |theme: &Theme, _status| channel_button_style(theme, is_soloed))
        .on_press(Event::track_solo_toggled(id));

    // 增益推子：线性 0..=2（1.0 = 0 dB），纵向推子。
    let gain = vertical_slider(0.0_f32..=2.0_f32, strip.gain, move |v| {
        Event::track_gain_changed(id, v)
    })
    .height(Length::Fixed(180.0))
    .step(0.01_f32);

    // 声像：-1..1，0 = 居中。
    let pan = slider(-1.0_f32..=1.0_f32, strip.pan, move |v| {
        Event::track_pan_changed(id, v)
    })
    .step(0.01_f32);

    column![name, row![mute_btn, solo_btn].spacing(4), gain, pan]
        .spacing(6)
        .width(Length::Shrink)
        .align_x(iced_core::Alignment::Center)
        .into()
}

/// 通道条 M/S 按钮样式（激活时高亮）
fn channel_button_style(theme: &Theme, active: bool) -> button::Style {
    let p = theme.extended_palette();
    button::Style {
        background: Some(Background::Color(if active {
            p.primary.strong.color
        } else {
            p.background.weak.color
        })),
        text_color: p.background.base.text,
        ..Default::default()
    }
}

/// 透明背景按钮（入口按钮 / 标题栏按钮）
fn transparent_button(theme: &Theme, _status: button::Status) -> button::Style {
    let p = theme.extended_palette();
    button::Style {
        text_color: p.background.base.text,
        ..Default::default()
    }
    .with_background(Color::TRANSPARENT)
}

/// 面板容器背景样式
fn panel_background(theme: &Theme) -> container::Style {
    let p = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(p.background.weak.color)),
        border: iced_core::border::Border {
            color: p.background.strongest.color,
            width: 1.0,
            radius: 6.0.into(),
        },
        ..Default::default()
    }
}
