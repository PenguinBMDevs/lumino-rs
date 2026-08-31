//! 右侧详情面板 — 对应 yinhe `right_panel/event_browser/detail.rs:1243`
//!
//! 按 `SelectedItem` 分发到各详情视图（与 yinhe `show_event_detail` 一致）：
//! - `Automation` 统一 CC / PitchBend / RPN / NRPN / Tempo
//! - `TimeSig` / `KeySig` / `Markers` / `Lyrics` / `Chord` / `Notes` / `ProgramChange`
//! - `ProjectJson` / `MappingJson` / `Overview` / `TrackDetail`
//!
//! 表格走 `table::view`（`scrollable` 虚拟化），编辑走 `edit::*_popups` 占位。

use iced_widget::{column, text};

use lumino_ui_core::{Element, Theme, window::Window};

use super::state::{EventBrowserState, JumpRequest, SelectedItem};
use super::table;

/// 详情面板 `view()` — 按 `SelectedItem` 分发（对齐 yinhe `show_event_detail`）
///
/// 返回 `Element` 的 iced 侧简化：跳转由外层 `Message` 驱动，此处仅产出
/// `Element`；`JumpRequest` 语义以文档注释保留，待 P6 接入
/// `EditorState.cursor_tick` 时打通。
pub fn view<'a>(
    window: &'a Window,
    item: Option<&'a SelectedItem>,
    state: &'a EventBrowserState,
    track_detail_name: Option<&'a str>,
) -> Element<'a> {
    match item {
        Some(SelectedItem::ProjectJson) => project_json_view(window),
        Some(SelectedItem::MappingJson) => mapping_json_view(window),
        Some(SelectedItem::TimeSig) => timesig_view(window, state),
        Some(SelectedItem::KeySig) => keysig_view(window, state),
        Some(SelectedItem::Markers) => text_events_view(window, state, "Markers".to_string()),
        Some(SelectedItem::ConductorLyrics) => {
            text_events_view(window, state, "Conductor Lyrics".to_string())
        }
        Some(SelectedItem::ConductorChord) => {
            text_events_view(window, state, "Conductor Chord".to_string())
        }
        Some(SelectedItem::Notes { track }) => notes_view(window, state, *track),
        Some(SelectedItem::ProgramChange { track }) => pc_view(window, state, *track),
        Some(SelectedItem::Automation { track, target }) => {
            automation_view(window, state, *track, target.display_name())
        }
        Some(SelectedItem::Lyrics { track }) => {
            text_events_view(window, state, format!("Lyrics (track {track})"))
        }
        Some(SelectedItem::Chord { track }) => {
            text_events_view(window, state, format!("Chord (track {track})"))
        }
        None => {
            if let Some(name) = track_detail_name {
                track_detail_view(window, name)
            } else {
                overview_view(window)
            }
        }
    }
}

fn section_title<'a>(window: &'a Window, title: String) -> Element<'a> {
    let palette = window.theme.extended_palette();
    text(title)
        .size(12)
        .style(move |_theme: &Theme| iced_widget::text::Style {
            color: Some(palette.background.strong.text),
        })
        .into()
}

fn automation_view<'a>(
    window: &'a Window,
    state: &'a EventBrowserState,
    _track: u16,
    target_name: String,
) -> Element<'a> {
    let headers = [
        ("#", 40.0),
        ("Tick", 70.0),
        ("Position", 80.0),
        ("Value", 60.0),
        ("X1", 50.0),
        ("Y1", 50.0),
        ("X2", 50.0),
        ("Y2", 50.0),
        ("Shape", 90.0),
    ];
    let widths = [40.0, 70.0, 80.0, 60.0, 50.0, 50.0, 50.0, 50.0, 90.0];
    let rows: Vec<Vec<String>> = (0..3)
        .map(|i| {
            vec![
                (i + 1).to_string(),
                (i * 480).to_string(),
                format!("{}/0", i + 1),
                "64".to_string(),
                "0.00".to_string(),
                "0.00".to_string(),
                "0.00".to_string(),
                "0.00".to_string(),
                "Linear".to_string(),
            ]
        })
        .collect();
    column![
        section_title(window, format!("{} ({} events)", target_name, rows.len())),
        table::view(window, &headers, rows, &widths, &[], state.event_page, 1),
    ]
    .spacing(6)
    .into()
}

fn timesig_view<'a>(window: &'a Window, state: &'a EventBrowserState) -> Element<'a> {
    let headers = [
        ("#", 40.0),
        ("Tick", 70.0),
        ("Position", 80.0),
        ("Numerator", 50.0),
        ("Denominator", 50.0),
    ];
    let widths = [40.0, 70.0, 80.0, 50.0, 50.0];
    let rows = vec![vec![
        "1".to_string(),
        "0".to_string(),
        "1/0".to_string(),
        "4".to_string(),
        "4".to_string(),
    ]];
    column![
        section_title(window, "TimeSig (1)".to_string()),
        table::view(window, &headers, rows, &widths, &[], state.event_page, 1),
    ]
    .spacing(6)
    .into()
}

fn keysig_view<'a>(window: &'a Window, state: &'a EventBrowserState) -> Element<'a> {
    let headers = [
        ("#", 40.0),
        ("Tick", 70.0),
        ("Position", 80.0),
        ("KeySig", 100.0),
        ("Root", 60.0),
        ("Scale", 80.0),
    ];
    let widths = [40.0, 70.0, 80.0, 100.0, 60.0, 80.0];
    let rows = vec![vec![
        "1".to_string(),
        "0".to_string(),
        "1/0".to_string(),
        "C Major".to_string(),
        "C (0)".to_string(),
        "Major".to_string(),
    ]];
    column![
        section_title(window, "KeySig (1)".to_string()),
        table::view(window, &headers, rows, &widths, &[], state.event_page, 1),
    ]
    .spacing(6)
    .into()
}

fn text_events_view<'a>(
    window: &'a Window,
    state: &'a EventBrowserState,
    label: String,
) -> Element<'a> {
    let headers = [
        ("#", 40.0),
        ("Tick", 70.0),
        ("Position", 80.0),
        ("Text", 200.0),
    ];
    let widths = [40.0, 70.0, 80.0, 200.0];
    let rows = vec![vec![
        "1".to_string(),
        "0".to_string(),
        "1/0".to_string(),
        "Hello".to_string(),
    ]];
    column![
        section_title(window, format!("{label} (1)")),
        table::view(window, &headers, rows, &widths, &[], state.event_page, 1),
    ]
    .spacing(6)
    .into()
}

fn notes_view<'a>(window: &'a Window, state: &'a EventBrowserState, track: u16) -> Element<'a> {
    let headers = [
        ("#", 40.0),
        ("id", 70.0),
        ("Tick", 70.0),
        ("Position", 80.0),
        ("Gate", 60.0),
        ("End", 80.0),
        ("End Pos", 90.0),
        ("Key", 50.0),
        ("Vel", 50.0),
    ];
    let widths = [40.0, 70.0, 70.0, 80.0, 60.0, 80.0, 90.0, 50.0, 50.0];
    let rows = vec![vec![
        "1".to_string(),
        "#1".to_string(),
        "0".to_string(),
        "1/0".to_string(),
        "480".to_string(),
        "480".to_string(),
        "1/480".to_string(),
        "60".to_string(),
        "100".to_string(),
    ]];
    column![
        section_title(window, format!("Notes (track {track})")),
        table::view(window, &headers, rows, &widths, &[], state.event_page, 1),
    ]
    .spacing(6)
    .into()
}

fn pc_view<'a>(window: &'a Window, state: &'a EventBrowserState, track: u16) -> Element<'a> {
    let headers = [
        ("#", 40.0),
        ("Tick", 70.0),
        ("Position", 80.0),
        ("Program", 50.0),
    ];
    let widths = [40.0, 70.0, 80.0, 50.0];
    let rows = vec![vec![
        "1".to_string(),
        "0".to_string(),
        "1/0".to_string(),
        "1".to_string(),
    ]];
    column![
        section_title(window, format!("Program Change (track {track})")),
        table::view(window, &headers, rows, &widths, &[], state.event_page, 1),
    ]
    .spacing(6)
    .into()
}

fn project_json_view<'a>(window: &'a Window) -> Element<'a> {
    let palette = window.theme.extended_palette();
    column![
        section_title(window, "project.json".to_string()),
        text("version / name / artist / ppq / compression_level …")
            .size(11)
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

fn mapping_json_view<'a>(window: &'a Window) -> Element<'a> {
    column![
        section_title(window, "mapping.json".to_string()),
        text("ports → channels → tracks (name / uuid / muted / soloed)").size(11),
    ]
    .spacing(6)
    .padding([8, 8])
    .into()
}

fn overview_view<'a>(window: &'a Window) -> Element<'a> {
    let palette = window.theme.extended_palette();
    column![
        text("Overview").size(13).style(move |_theme: &Theme| {
            iced_widget::text::Style {
                color: Some(palette.background.strong.text),
            }
        }),
        text("Name / Artist / PPQ / Tracks / Notes / CC / Tempo …").size(11),
        text("← Select an item on the left to see details")
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

fn track_detail_view<'a>(window: &'a Window, name: &str) -> Element<'a> {
    column![
        section_title(window, format!("Track: {name}")),
        text("UUID / Port / Channel / Color / Muted / Solo").size(11),
        text("Notes / CC / PitchBend / Program Change counts").size(11),
    ]
    .spacing(6)
    .padding([8, 8])
    .into()
}

/// 供外层 `event_browser::view` 构造跳转请求的占位（对齐 yinhe `take_row_click`）
///
/// iced 桩由 `Message` 直接携带 `JumpRequest`，此处仅保留转换工具。
#[must_use]
pub fn jump_request_for_tick(tick: u32, note: Option<(u16, u8)>) -> JumpRequest {
    JumpRequest { tick, note }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_ui_core::window::Window;

    #[test]
    fn detail_view_variants_do_not_panic() {
        let window = Window::new("Tokyo Night Storm");
        let state = EventBrowserState::default();
        let _ = view(&window, Some(&SelectedItem::TimeSig), &state, None);
        let _ = view(
            &window,
            Some(&SelectedItem::Notes { track: 0 }),
            &state,
            None,
        );
        let _ = view(&window, None, &state, Some("Piano"));
        let _ = view(&window, None, &state, None);
    }
}
