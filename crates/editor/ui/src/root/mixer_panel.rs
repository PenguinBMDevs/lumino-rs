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
use crate::sidebar::MIXER_DEFAULT_VOLUME;
use crate::{Element, Theme};
use iced_core::{Background, Length, Padding, alignment};
use iced_widget::{Space, button, column, container, mouse_area, row, scrollable, text};
use lumino_ui_core::sidebar_event::Event;

mod strips;
pub(crate) use strips::{build_master_strip, build_strip, transparent_button};

/// 单个通道条固定宽度（逻辑像素），用于横向滚动时按索引确定 x 位置并做视口裁剪。
const STRIP_WIDTH: f32 = 88.0;
/// 通道条横向间距
const STRIP_SPACING: f32 = 8.0;
/// 相邻通道条步进（宽度 + 间距）
const STRIP_STEP: f32 = STRIP_WIDTH + STRIP_SPACING;
/// 推子 / 标尺 / 电平表统一高度
const FADER_HEIGHT: f32 = 180.0;
/// 电平呈现条宽度
const METER_WIDTH: f32 = 6.0;
/// 视口裁剪外扩缓冲：保证"单个混音控制器的任何区域进入可视范围即显示"（宁多勿漏）。
const CULL_MARGIN: f32 = STRIP_STEP * 2.0;
/// 面板最大宽度（与 `max_width(900)` 一致），作为可见宽度的安全上界。
const PANEL_VISIBLE_WIDTH: f32 = 900.0;

/// 浮动混音台面板状态
#[derive(Debug, Clone)]
pub(crate) struct MixerPanelState {
    /// 面板是否打开
    pub open: bool,
    /// 面板主体是否展开（最大化）；false = 仅显示标题栏（最小化）
    pub maximized: bool,
    /// 面板距左/底边界内缩（拖拽累加，逻辑像素）。
    /// `offset.0` = 距左边界内缩（= 面板左缘 x），`offset.1` = 距底边界内缩。
    pub offset: (f32, f32),
    /// 是否正在拖拽（标题栏按下且未松开）
    pub(crate) dragging: bool,
    /// 拖拽期间上一帧的绝对光标位置（相对全窗口覆盖层）；用于计算增量，
    /// 使面板在光标离开标题栏/窗口范围时仍跟随（首次 move 时为 None 仅记录）。
    pub(crate) last_cursor: Option<(f32, f32)>,
    /// 混音台主音量（全局音量控制器），0..=127（127 对应 0 dB）
    pub(crate) master_volume: u8,
    /// 混音台主体横向滚动偏移（逻辑像素），用于视口裁剪节流
    pub(crate) scroll_x: f32,
}

impl Default for MixerPanelState {
    fn default() -> Self {
        Self {
            open: false,
            // 默认展开主体（显示各音轨推子）；仅标题栏的紧凑态需用户收起。
            maximized: true,
            // 默认显形位置：紧贴左侧栏右侧（48px 栏宽 + 8px 间隙）、距底 8px，
            // 避免渲染在 (0,0) 被左栏与状态栏遮住而"看起来没弹出"。
            offset: (56.0, 8.0),
            dragging: false,
            last_cursor: None,
            // 主音量出厂默认 100（与单轨默认一致）。
            master_volume: MIXER_DEFAULT_VOLUME,
            scroll_x: 0.0,
        }
    }
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
    let panel_outer = container(panel)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(alignment::Horizontal::Left)
        .align_y(alignment::Vertical::Bottom)
        .padding(Padding {
            top: 0.0,
            right: 0.0,
            bottom: root.mixer_panel.offset.1,
            left: root.mixer_panel.offset.0,
        });

    // 拖拽进行中：叠加全窗口透明 mouse_area，使光标离开标题栏甚至窗口范围时
    // 仍能持续收到 on_move（Windows 按住期间 OS 隐式捕获鼠标，窗口外亦触发），
    // 实现"始终跟随鼠标"的拖拽；松开由覆盖层的 on_release 结束拖拽。
    if root.mixer_panel.dragging {
        let drag_overlay = mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
            .on_move(|p| Event::mixer_panel_dragged(p.x, p.y))
            .on_release(Event::mixer_panel_drag_ended());
        return Some(
            iced_widget::Stack::new()
                .push(panel_outer)
                .push(drag_overlay)
                .into(),
        );
    }

    Some(panel_outer.into())
}

/// 标题栏：含关闭按钮；标题+中间空白为拖拽手柄（按下开始拖拽，全窗口覆盖层接管后续移动）。
fn build_header() -> Element<'static> {
    let title = text("混音台").size(13);
    let close_btn = button(text("✕").size(12))
        .padding(2)
        .style(transparent_button)
        .on_press(Event::mixer_panel_toggled());
    let controls = row![close_btn].spacing(4);

    // 拖拽手柄：标题 + 中间空白区域；仅 on_press 开始拖拽，后续移动由全窗口
    // 覆盖层（view_mixer_panel 中 dragging 时叠加）接管，确保跟随光标不中断。
    let drag_handle = mouse_area(
        row![title, Space::new().width(Length::Fill)]
            .spacing(8)
            .align_y(iced_core::Alignment::Center),
    )
    .on_press(Event::mixer_panel_drag_started());

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
///
/// 渲染序列首项为**全局音量控制器（主音量）**，其后为各普通音轨通道条；
/// 指挥轨（conductor）不发音符、无增益/声像，不属于混音对象，跳过。
///
/// **视口节流**：不在可见范围内的通道条以固定宽高的占位 `Space` 代替——既不显示、
/// 也不构建重控件（推子/按钮/电平表），同时维持内容总宽恒定，使滚动位置稳定、
/// 不产生抖动。判定采用"任何区域进入可见范围即显示"（外扩 `CULL_MARGIN` 缓冲，
/// 宁多勿漏）。
fn build_body(root: &Root) -> Element<'static> {
    let scroll_x = root.mixer_panel.scroll_x;
    let visible_start = scroll_x - CULL_MARGIN;
    let visible_end = scroll_x + PANEL_VISIBLE_WIDTH + CULL_MARGIN;

    let mut rendered: Vec<Element<'static>> = Vec::new();

    // 主音量（索引 0）：全局音量控制器，始终位于首项。
    rendered.push(if strip_in_view(0, visible_start, visible_end) {
        build_master_strip(root)
    } else {
        placeholder_strip()
    });

    for (i, track) in root
        .sidebar
        .tracks
        .iter()
        .filter(|track| !track.is_conductor)
        .enumerate()
    {
        let idx = i + 1;
        rendered.push(if strip_in_view(idx, visible_start, visible_end) {
            build_strip(root, track)
        } else {
            placeholder_strip()
        });
    }

    if rendered.is_empty() {
        return container(text("暂无音轨").size(12)).padding(16).into();
    }

    scrollable(row(rendered).spacing(STRIP_SPACING).padding(8))
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new().width(8),
        ))
        .on_scroll(|vp| Event::mixer_panel_scrolled(vp.absolute_offset().x))
        .into()
}

/// 判断索引为 `idx` 的通道条（固定宽度、按 `idx * STRIP_STEP` 排列）是否与可见区间相交。
/// 相交即意为"其任何区域已进入可视范围"，应显示。
fn strip_in_view(idx: usize, visible_start: f32, visible_end: f32) -> bool {
    let x_left = idx as f32 * STRIP_STEP;
    let x_right = x_left + STRIP_WIDTH;
    x_right > visible_start && x_left < visible_end
}

/// 不可见通道条的占位（固定宽高、无内容），仅用于维持滚动内容与裁剪边界稳定。
fn placeholder_strip() -> Element<'static> {
    Space::new()
        .width(Length::Fixed(STRIP_WIDTH))
        .height(Length::Fixed(FADER_HEIGHT))
        .into()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_in_view_basic() {
        // 索引 0 的条：左缘 0、右缘 STRIP_WIDTH(88)，与 [-100,100) 相交。
        assert!(strip_in_view(0, -100.0, 100.0));
        // 索引 5 的条：左缘 5*96=480，超出 [0,10) 区间，不相交。
        assert!(!strip_in_view(5, 0.0, 10.0));
        // 与可见区间左边界刚好相切（右缘 == visible_start）视为不相交。
        assert!(!strip_in_view(0, STRIP_WIDTH, 200.0));
    }
}
