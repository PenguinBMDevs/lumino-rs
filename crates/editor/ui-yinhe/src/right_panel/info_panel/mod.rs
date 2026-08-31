//! Info 面板入口 — 对应 yinhe `right_panel/info_panel.rs:123`
//!
//! 按 `InfoContent` 分发：
//! - `anchor`  — 自动化锚点（Tick / Value / Shape / X1/Y1/X2/Y2）
//! - `track`   — 音轨信息（名称 / 端口 / 通道 / Mute/Solo / 摘要）
//! - `selection` — 选框信息（PR/AR/AM 统计 + 批量表达式编辑 + 变速 + 翻转）
//! - `None`    — 回落 `project_info`（与 yinhe `None => project_info::show` 一致）

pub mod anchor;
pub mod selection;
pub mod track;

pub use anchor::{AnchorInfo, AnchorShape};
pub use selection::{AmSelectionState, SelectionSummary, SelectionViewKind};
pub use track::{TrackInfo, TrackPanelInfoState};

use iced_widget::{column, text};

use lumino_ui_core::{Element, Theme, window::Window};

/// Info 面板内容判别（对齐 yinhe `InfoContent`）
///
/// yinhe 原 `InfoContent::Anchor { track_idx, lane_idx, event_idx, target }`
/// `Track` / `None`（无选中时显示工程设置）。
#[derive(Debug, Clone, PartialEq)]
pub enum InfoContent {
    Anchor(AnchorInfo),
    Track(u16),
    None,
}

impl Default for InfoContent {
    fn default() -> Self {
        Self::None
    }
}

/// Info 面板聚合状态（供右侧面板持有）
///
/// 对齐 yinhe `Document.edit: track_selected / anchor_selected / pending_edits` 等
/// 分散状态的 iced 侧聚合；`has_selection` 优先于 `content`（与
/// `selection::has_any_selection` 判定一致）。
#[derive(Debug, Clone, Default)]
pub struct InfoPanelState {
    pub content: InfoContent,
    pub track: TrackPanelInfoState,
    pub selection: SelectionSummary,
    pub anchor: Option<AnchorInfo>,
    pub has_selection: bool,
    pub am_state: Option<AmSelectionState>,
}

/// Info 面板 `view()` — 选框优先，其次按 `InfoContent` 分发
///
/// ```text
/// if has_selection { selection::view }
/// else match content {
///   Anchor(a) => anchor::view,
///   Track(idx) => track::view,
///   None => project_info 回落占位
/// }
/// ```
pub fn view<'a>(window: &'a Window, state: &'a InfoPanelState) -> Element<'a> {
    if state.has_selection {
        return selection::view(window, &state.selection, state.am_state.as_ref());
    }

    match &state.content {
        InfoContent::Anchor(info) => anchor::view(window, info),
        InfoContent::Track(idx) => track::view(window, &state.track, *idx),
        InfoContent::None => fallback_project_info(window),
    }
}

fn fallback_project_info<'a>(window: &'a Window) -> Element<'a> {
    let palette = window.theme.extended_palette();
    column![
        text("Project Info").size(13).style(move |_theme: &Theme| {
            iced_widget::text::Style {
                color: Some(palette.background.strong.text),
            }
        }),
        text("No selection — showing project settings")
            .size(11)
            .style(move |_theme: &Theme| {
                iced_widget::text::Style {
                    color: Some(palette.background.base.text),
                }
            }),
        text("Name / Artist / PPQ / Description … (see project_info panel)")
            .size(10)
            .style(move |_theme: &Theme| {
                iced_widget::text::Style {
                    color: Some(palette.background.weak.text),
                }
            }),
    ]
    .spacing(6)
    .padding([8, 8])
    .into()
}

/// 供上层 `Host` 调用的 skip 掩码计算占位（对齐 yinhe `track::send_skip_tracks`）
///
/// iced 桩不直接持有 `CpalAudioHandle`，仅保留签名约束；实际音频重建由
/// `Message::AudioAction` 驱动。
#[must_use]
pub fn compute_skip_mask_stub(_track_overrides: &[(bool, bool)]) -> u32 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_ui_core::window::Window;

    #[test]
    fn info_panel_view_no_selection_track() {
        let window = Window::new("Tokyo Night Storm");
        let mut state = InfoPanelState::default();
        state.content = InfoContent::Track(0);
        let _el = view(&window, &state);
    }

    #[test]
    fn info_panel_view_anchor() {
        let window = Window::new("Tokyo Night Storm");
        let mut state = InfoPanelState::default();
        state.content = InfoContent::Anchor(AnchorInfo {
            track_idx: 0,
            lane_idx: 0,
            tick: 480,
            value: 64.0,
            shape: AnchorShape::Step,
            target_name: "CC 7".to_string(),
            max_value: 127.0,
        });
        let _el = view(&window, &state);
    }

    #[test]
    fn info_panel_view_selection_priority() {
        let window = Window::new("Tokyo Night Storm");
        let mut state = InfoPanelState::default();
        state.has_selection = true;
        state.selection = SelectionSummary {
            count: 3,
            tick_span: (0.0, 1920.0),
            ..Default::default()
        };
        let _el = view(&window, &state);
    }
}
