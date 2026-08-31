//! 事件浏览器入口 — 对应 yinhe `right_panel/event_browser.rs:232`
//!
//! 模块结构（与 yinhe 一一对应，egui → iced）：
//! - `state` — `EventBrowserState` / `SelectedItem` / `JumpRequest`
//! - `tree` — 左侧树（`column + button`，非 `egui::ScrollArea`）
//! - `table` — 表格（`scrollable` 虚拟化，非 `egui_extras::TableBuilder`）
//! - `detail` — 右侧详情（按 `SelectedItem` 分发）
//! - `edit` — 右键编辑 popup（value / shape / tick / position）
//! - `bar_lookup` 语义内联于 `table::paginate` 的位置格式化占位
//!
//! 布局：上树下表的 `split_ratio` 分割（与 yinhe `split_handle::horizontal` 对齐，
//! iced 桩以 `container` + 比例容器占位，拖动由上层 `Message` 驱动）。

pub mod detail;
pub mod edit;
pub mod state;
pub mod table;
pub mod tree;

pub use state::{AutomationTarget, EventBrowserState, JumpRequest, SelectedItem};
pub use tree::{TreeModel, TreeTrackSummary};

use iced_core::Length;
use iced_widget::{column, container, scrollable, text};

use lumino_ui_core::{Element, Theme, window::Window};

/// 渲染事件浏览器（上下分割：树 + 详情）
///
/// ```text
/// column![
///   scrollable(tree::view),
///   split_handle (占位),
///   scrollable(detail::view)
/// ]
/// ```
/// 不画双层背景（`right_panel` 已铺 `app_bg`），仅保留内边距；
/// `split_ratio` 拖动与双击还原由 `Message` 在 Host 层处理。
pub fn view<'a>(window: &'a Window, state: &'a EventBrowserState) -> Element<'a> {
    let palette = window.theme.extended_palette();

    // 构造最小 TreeModel 占位（无文档时显示空树 + Overview）
    let tree_model = TreeModel {
        tempo_count: 1,
        time_sig_count: 1,
        ..Default::default()
    };

    let tree_el = scrollable(tree::view(window, tree_model, state))
        .height(Length::FillPortion(4))
        .width(Length::Fill);

    let split_handle = container(text("─").size(8))
        .width(Length::Fill)
        .height(Length::Fixed(6.0))
        .style(move |_theme: &Theme| container::Style {
            background: Some(iced_core::Background::Color(palette.background.weak.color)),
            ..Default::default()
        });

    let detail_el = scrollable(detail::view(
        window,
        state.selected_item.as_ref(),
        state,
        state.selected_track.and_then(|_| Some("Track detail")),
    ))
    .height(Length::FillPortion(6))
    .width(Length::Fill);

    column![tree_el, split_handle, detail_el]
        .spacing(0)
        .padding([4, 4])
        .into()
}

/// 共享 helper（供 `tree.rs` / `detail.rs` 复用，与 yinhe `event_browser.rs` 末尾三函数对齐）

#[must_use]
pub fn cc_label(controller: u8) -> &'static str {
    match controller {
        0 => "Bank Select MSB",
        1 => "Modulation",
        7 => "Volume",
        10 => "Pan",
        11 => "Expression",
        64 => "Sustain",
        91 => "Reverb",
        93 => "Chorus",
        _ => "",
    }
}

#[must_use]
pub fn port_letter(port: u8) -> char {
    if port < 26 {
        (b'A' + port) as char
    } else {
        '?'
    }
}

#[must_use]
pub fn group_tracks_by_port_channel(
    tracks: &[TreeTrackSummary],
    conductor_idx: Option<u16>,
) -> std::collections::BTreeMap<u8, std::collections::BTreeMap<u8, Vec<u16>>> {
    let mut out: std::collections::BTreeMap<u8, std::collections::BTreeMap<u8, Vec<u16>>> =
        std::collections::BTreeMap::new();
    for t in tracks {
        if Some(t.index) == conductor_idx {
            continue;
        }
        out.entry(t.port)
            .or_default()
            .entry(t.channel)
            .or_default()
            .push(t.index);
    }
    out
}

/// Tick → 小节位置格式化的最小占位（对齐 yinhe `BarLookup::format`）
///
/// 完整实现需 `YinModel.tempo_map` / `conductor.time_sig` 分段计算；
/// iced 桩以 PPQ 固定 480、4/4 的简化公式占位，待 `lumino-midi-model` 接入后替换。
#[must_use]
pub fn format_tick_as_position(tick: u32, ppq: u32) -> String {
    let ppq = ppq.max(1);
    let ticks_per_bar = ppq * 4;
    let bar = tick / ticks_per_bar + 1;
    let tick_in_bar = tick % ticks_per_bar;
    format!("{bar}/{tick_in_bar}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_ui_core::window::Window;

    #[test]
    fn event_browser_view_does_not_panic() {
        let window = Window::new("Tokyo Night Storm");
        let state = EventBrowserState::default();
        let _el = view(&window, &state);
    }

    #[test]
    fn port_letter_basic() {
        assert_eq!(port_letter(0), 'A');
        assert_eq!(port_letter(15), 'P');
        assert_eq!(port_letter(25), 'Z');
        assert_eq!(port_letter(26), '?');
    }

    #[test]
    fn format_tick_position() {
        assert_eq!(format_tick_as_position(0, 480), "1/0");
        assert_eq!(format_tick_as_position(1920, 480), "2/0");
    }
}
