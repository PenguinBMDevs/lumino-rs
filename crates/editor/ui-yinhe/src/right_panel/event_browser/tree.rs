//! 左侧树状导航 — 对应 yinhe `right_panel/event_browser/tree.rs:507`
//!
//! yinhe 原 `render_tree` 用 `egui::Ui` 手绘行背景 + `interact` 抢占 hover；
//! iced 桩用 `column + button` 重构，保留：
//! - project/mapping → Conductor → Port/Channel/Track 三级分组
//! - Track 展开后 Notes / Automation lane / Program Change / Lyrics / Chord
//! - 选中高亮（`selected_bg`）与 hover 反馈
//! - `ArchiveKey` 展开折叠

use std::collections::BTreeMap;

use iced_core::{Alignment, Length};
use iced_widget::{button, column, row, text};

use lumino_ui_core::{Element, Theme, window::Window};

use super::state::{ArchiveKey, AutomationTarget, EventBrowserState, SelectedItem};

const ROW_H: f32 = 22.0;
const INDENT: f32 = 14.0;

/// Track 摘要（供 tree 行显示，对齐 yinhe `TrackData` 精简）
#[derive(Debug, Clone)]
pub struct TreeTrackSummary {
    pub index: u16,
    pub name: String,
    pub port: u8,
    pub channel: u8,
    pub note_count: usize,
    pub automation_lanes: Vec<AutomationTarget>,
    pub pc_count: usize,
    pub lyrics_count: usize,
    pub chord_count: usize,
}

/// 树数据源（对齐 yinhe `YinModel` + `group_tracks_by_port_channel` 的输入）
#[derive(Debug, Clone, Default)]
pub struct TreeModel {
    pub tempo_count: usize,
    pub time_sig_count: usize,
    pub key_sig_count: usize,
    pub markers_count: usize,
    pub lyrics_count: usize,
    pub chord_count: usize,
    pub tracks: Vec<TreeTrackSummary>,
    pub conductor_idx: Option<u16>,
}

fn port_letter(port: u8) -> char {
    if port < 26 {
        (b'A' + port) as char
    } else {
        '?'
    }
}

fn group_tracks_by_port_channel(
    tracks: &[TreeTrackSummary],
    conductor_idx: Option<u16>,
) -> BTreeMap<u8, BTreeMap<u8, Vec<u16>>> {
    let mut out: BTreeMap<u8, BTreeMap<u8, Vec<u16>>> = BTreeMap::new();
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

fn leaf_button<'a>(window: &'a Window, label: String, depth: usize, selected: bool) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let _pad_left = depth as f32 * INDENT + INDENT;

    button(
        row![
            text("•").size(10).style(move |_theme: &Theme| {
                iced_widget::text::Style {
                    color: Some(if selected {
                        palette.background.strong.text
                    } else {
                        palette.background.weak.text
                    }),
                }
            }),
            text(label).size(11).style(move |_theme: &Theme| {
                iced_widget::text::Style {
                    color: Some(if selected {
                        palette.background.strong.text
                    } else {
                        palette.background.base.text
                    }),
                }
            }),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .padding([2, 6]),
    )
    .padding([0, 0])
    .width(Length::Fill)
    .style(move |_theme: &Theme, status| {
        let bg = if selected {
            palette.background.strong.color
        } else if status == button::Status::Hovered {
            palette.background.weak.color
        } else {
            iced_core::Color::TRANSPARENT
        };
        button::Style {
            background: Some(iced_core::Background::Color(bg)),
            border: iced_core::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    })
    .into()
}

fn dir_button<'a>(
    window: &'a Window,
    label: String,
    _depth: usize,
    expanded: bool,
    child_count: usize,
) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let chev = if expanded { "▼" } else { "▶" };
    let folder = if expanded { "📂" } else { "📁" };

    button(
        row![
            text(chev).size(10),
            text(folder).size(11),
            text(label).size(11),
            text(format!("({child_count})"))
                .size(10)
                .style(move |_theme: &Theme| {
                    iced_widget::text::Style {
                        color: Some(palette.background.weak.text),
                    }
                }),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .padding([2, 6]),
    )
    .padding([0, 0])
    .width(Length::Fill)
    .style(move |_theme: &Theme, status| {
        let bg = if status == button::Status::Hovered {
            palette.background.weak.color
        } else {
            iced_core::Color::TRANSPARENT
        };
        button::Style {
            background: Some(iced_core::Background::Color(bg)),
            border: iced_core::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    })
    .into()
}

/// 渲染整个树状导航（`column + button`，无 `egui::ScrollArea`）
///
/// 外层 `event_browser::view` 已包 `scrollable`，此处仅产出 `column`。
pub fn view<'a>(window: &'a Window, model: TreeModel, state: &'a EventBrowserState) -> Element<'a> {
    let mut items: Vec<Element<'a>> = Vec::new();

    // project / mapping
    items.push(leaf_button(
        window,
        "project.json".to_string(),
        0,
        state.selected_item == Some(SelectedItem::ProjectJson),
    ));
    items.push(leaf_button(
        window,
        "mapping.json".to_string(),
        0,
        state.selected_item == Some(SelectedItem::MappingJson),
    ));

    // Conductor
    let cond_expanded = state.expanded_keys.contains(&ArchiveKey::Conductor);
    items.push(dir_button(
        window,
        "Conductor".to_string(),
        0,
        cond_expanded,
        6,
    ));
    if cond_expanded {
        items.push(leaf_button(
            window,
            format!("Tempo ({})", model.tempo_count),
            1,
            state.selected_item
                == Some(SelectedItem::Automation {
                    track: 0,
                    target: AutomationTarget::Tempo,
                }),
        ));
        items.push(leaf_button(
            window,
            format!("TimeSig ({})", model.time_sig_count),
            1,
            state.selected_item == Some(SelectedItem::TimeSig),
        ));
        items.push(leaf_button(
            window,
            format!("KeySig ({})", model.key_sig_count),
            1,
            state.selected_item == Some(SelectedItem::KeySig),
        ));
        items.push(leaf_button(
            window,
            format!("Markers ({})", model.markers_count),
            1,
            state.selected_item == Some(SelectedItem::Markers),
        ));
        items.push(leaf_button(
            window,
            format!("Lyrics ({})", model.lyrics_count),
            1,
            state.selected_item == Some(SelectedItem::ConductorLyrics),
        ));
        items.push(leaf_button(
            window,
            format!("Chord ({})", model.chord_count),
            1,
            state.selected_item == Some(SelectedItem::ConductorChord),
        ));
    }

    // Port / Channel / Track
    let groups = group_tracks_by_port_channel(&model.tracks, model.conductor_idx);
    for (&port, channels) in &groups {
        let port_expanded = state.expanded_keys.contains(&ArchiveKey::Port(port));
        let port_track_count: usize = channels.values().map(|v| v.len()).sum();
        items.push(dir_button(
            window,
            format!("Port {} ({} tracks)", port_letter(port), port_track_count),
            0,
            port_expanded,
            channels.len(),
        ));
        if !port_expanded {
            continue;
        }
        for (&channel, track_indices) in channels {
            let ch_expanded = state
                .expanded_keys
                .contains(&ArchiveKey::Channel(port, channel));
            items.push(dir_button(
                window,
                format!("Ch {} ({} tracks)", channel + 1, track_indices.len()),
                1,
                ch_expanded,
                track_indices.len(),
            ));
            if !ch_expanded {
                continue;
            }
            for &idx in track_indices {
                if let Some(track) = model.tracks.iter().find(|t| t.index == idx).cloned() {
                    let is_expanded = state.expanded_keys.contains(&ArchiveKey::Track(idx));
                    items.push(track_row(window, track.clone(), state));
                    if is_expanded {
                        items.extend(track_children(window, track, state));
                    }
                }
            }
        }
    }

    column(items).spacing(1).padding([2, 2]).into()
}

fn track_row<'a>(
    window: &'a Window,
    track: TreeTrackSummary,
    state: &'a EventBrowserState,
) -> Element<'a> {
    let expanded = state
        .expanded_keys
        .contains(&ArchiveKey::Track(track.index));
    let is_selected = state.selected_track == Some(track.index);
    let chev = if expanded { "▼" } else { "▶" };
    let label = if track.name.is_empty() {
        format!("(track #{})", track.index)
    } else {
        track.name.clone()
    };
    let summary = format!(
        "{} notes · {} auto · {} PC",
        track.note_count,
        track.automation_lanes.len(),
        track.pc_count
    );
    let palette = window.theme.extended_palette();

    button(
        row![
            text(chev).size(10),
            text("♫").size(11),
            text(label).size(11).style(move |_theme: &Theme| {
                iced_widget::text::Style {
                    color: Some(if is_selected {
                        palette.background.strong.text
                    } else {
                        palette.background.base.text
                    }),
                }
            }),
            text(format!("[{summary}]"))
                .size(10)
                .style(move |_theme: &Theme| {
                    iced_widget::text::Style {
                        color: Some(if is_selected {
                            palette.background.strong.text
                        } else {
                            palette.background.weak.text
                        }),
                    }
                }),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .padding([2, 6]),
    )
    .width(Length::Fill)
    .style(move |_theme: &Theme, status| {
        let bg = if is_selected {
            palette.background.strong.color
        } else if status == button::Status::Hovered {
            palette.background.weak.color
        } else {
            iced_core::Color::TRANSPARENT
        };
        button::Style {
            background: Some(iced_core::Background::Color(bg)),
            border: iced_core::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    })
    .into()
}

fn track_children<'a>(
    window: &'a Window,
    track: TreeTrackSummary,
    state: &'a EventBrowserState,
) -> Vec<Element<'a>> {
    let mut out = Vec::new();
    out.push(leaf_button(
        window,
        format!("Notes ({})", track.note_count),
        3,
        state.selected_item == Some(SelectedItem::Notes { track: track.index }),
    ));
    for lane in &track.automation_lanes {
        out.push(leaf_button(
            window,
            format!("{} ({} events)", lane.display_name(), 0),
            3,
            state.selected_item
                == Some(SelectedItem::Automation {
                    track: track.index,
                    target: lane.clone(),
                }),
        ));
    }
    out.push(leaf_button(
        window,
        format!("Program Change ({})", track.pc_count),
        3,
        state.selected_item == Some(SelectedItem::ProgramChange { track: track.index }),
    ));
    out.push(leaf_button(
        window,
        format!("Lyrics ({})", track.lyrics_count),
        3,
        state.selected_item == Some(SelectedItem::Lyrics { track: track.index }),
    ));
    out.push(leaf_button(
        window,
        format!("Chord ({})", track.chord_count),
        3,
        state.selected_item == Some(SelectedItem::Chord { track: track.index }),
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_ui_core::window::Window;

    #[test]
    fn tree_view_empty_does_not_panic() {
        let window = Window::new("Tokyo Night Storm");
        let model = TreeModel::default();
        let state = EventBrowserState::default();
        let _el = view(&window, model, &state);
    }

    #[test]
    fn tree_view_with_tracks() {
        let window = Window::new("Tokyo Night Storm");
        let model = TreeModel {
            tempo_count: 2,
            tracks: vec![
                TreeTrackSummary {
                    index: 0,
                    name: "Piano".to_string(),
                    port: 0,
                    channel: 0,
                    note_count: 10,
                    automation_lanes: vec![AutomationTarget::Cc { controller: 7 }],
                    pc_count: 1,
                    lyrics_count: 0,
                    chord_count: 0,
                },
                TreeTrackSummary {
                    index: 1,
                    name: "Bass".to_string(),
                    port: 0,
                    channel: 1,
                    note_count: 5,
                    automation_lanes: vec![],
                    pc_count: 0,
                    lyrics_count: 2,
                    chord_count: 1,
                },
            ],
            ..Default::default()
        };
        let mut state = EventBrowserState::default();
        state.expanded_keys.insert(ArchiveKey::Port(0));
        state.expanded_keys.insert(ArchiveKey::Channel(0, 0));
        state.expanded_keys.insert(ArchiveKey::Track(0));
        let _el = view(&window, model, &state);
    }
}
