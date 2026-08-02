//! 事件浏览器左侧树的数据查询逻辑。
//!
//! 本模块只产生渲染所需的行数据，不包含任何 egui 或 iced Canvas 绘制代码。

use std::collections::BTreeMap;
use std::sync::Arc;

use lumino_note_core::automation::AutomationLane;
use lumino_note_core::event::AutomationTarget;

use crate::sidebar::core::Track;

use super::state::{ArchiveKey, SelectedItem};

mod automation;

/// 树中的一行数据。
#[derive(Clone, Debug, PartialEq)]
pub enum TreeItem {
    /// 可展开/折叠的目录根节点。
    Root { name: String, key: ArchiveKey },
    /// 不可展开的叶子节点。
    Leaf {
        name: String,
        depth: u8,
        item: SelectedItem,
    },
    /// 音轨行（可展开显示其下的叶子）。
    Track {
        id: u16,
        name: String,
        port: u8,
        channel: u8,
        depth: u8,
    },
}

/// 收集左侧树所需的所有行数据。
///
/// 树结构：
/// - project.json（叶子）
/// - mapping.json（叶子）
/// - Conductor（根）
///   - Tempo（叶子）
///   - TimeSig（叶子）
///   - KeySig（叶子）
///   - Markers（叶子）
///   - Lyrics（叶子）
///   - Chord（叶子）
/// - Port A / B / ...（根）
///   - Channel 01 / 02 / ...（根）
///     - Track name（音轨行）
///       - Notes（叶子）
///       - Automation lanes（叶子）
///       - Program Change（叶子）
///       - Lyrics（叶子）
///       - Chord（叶子）
#[allow(clippy::vec_init_then_push)] // 树项分段 push，结构更清晰
pub(super) fn collect_tree_items(tracks: &[Track], lanes: &[Arc<AutomationLane>]) -> Vec<TreeItem> {
    let mut items = Vec::new();

    items.push(TreeItem::Leaf {
        name: "project.json".to_string(),
        depth: 0,
        item: SelectedItem::ProjectJson,
    });
    items.push(TreeItem::Leaf {
        name: "mapping.json".to_string(),
        depth: 0,
        item: SelectedItem::MappingJson,
    });

    items.push(TreeItem::Root {
        name: "Conductor".to_string(),
        key: ArchiveKey::Conductor,
    });
    // Conductor 子项始终显示，depth = 1
    items.push(TreeItem::Leaf {
        name: "Tempo".to_string(),
        depth: 1,
        item: SelectedItem::Automation {
            track: 0,
            target: AutomationTarget::Tempo,
        },
    });
    items.push(TreeItem::Leaf {
        name: "TimeSig".to_string(),
        depth: 1,
        item: SelectedItem::TimeSig,
    });
    items.push(TreeItem::Leaf {
        name: "KeySig".to_string(),
        depth: 1,
        item: SelectedItem::KeySig,
    });
    items.push(TreeItem::Leaf {
        name: "Markers".to_string(),
        depth: 1,
        item: SelectedItem::Markers,
    });
    items.push(TreeItem::Leaf {
        name: "Lyrics".to_string(),
        depth: 1,
        item: SelectedItem::ConductorLyrics,
    });
    items.push(TreeItem::Leaf {
        name: "Chord".to_string(),
        depth: 1,
        item: SelectedItem::ConductorChord,
    });

    let groups = group_tracks_by_port_channel(tracks);
    for (&port, channels) in &groups {
        let port_track_count: usize = channels.values().map(|v| v.len()).sum();
        items.push(TreeItem::Root {
            name: port_root_name(port, port_track_count).to_string(),
            key: ArchiveKey::Port(port),
        });

        for (&channel, track_indices) in channels {
            items.push(TreeItem::Root {
                name: channel_root_name(channel, track_indices.len()),
                key: ArchiveKey::Channel(port, channel),
            });

            for &track_idx in track_indices {
                if let Some(track) = tracks.get(track_idx as usize) {
                    items.push(TreeItem::Track {
                        id: track_idx,
                        name: track_name(track),
                        port,
                        channel,
                        depth: 2,
                    });

                    // Track 下的叶子 depth = 3
                    items.push(TreeItem::Leaf {
                        name: "Notes".to_string(),
                        depth: 3,
                        item: SelectedItem::Notes { track: track_idx },
                    });

                    // 该音轨的 automation lanes（每条 lane 一个叶子）
                    items.extend(automation::collect_automation_items(track_idx, lanes));

                    items.push(TreeItem::Leaf {
                        name: "Program Change".to_string(),
                        depth: 3,
                        item: SelectedItem::ProgramChange { track: track_idx },
                    });
                    items.push(TreeItem::Leaf {
                        name: "Lyrics".to_string(),
                        depth: 3,
                        item: SelectedItem::Lyrics { track: track_idx },
                    });
                    items.push(TreeItem::Leaf {
                        name: "Chord".to_string(),
                        depth: 3,
                        item: SelectedItem::Chord { track: track_idx },
                    });
                }
            }
        }
    }

    items
}

/// 按 Port / Channel 对非 conductor 音轨分组。
fn group_tracks_by_port_channel(tracks: &[Track]) -> BTreeMap<u8, BTreeMap<u8, Vec<u16>>> {
    let mut out: BTreeMap<u8, BTreeMap<u8, Vec<u16>>> = BTreeMap::new();
    for (i, track) in tracks.iter().enumerate() {
        if track.is_conductor {
            continue;
        }
        let idx = i as u16;
        out.entry(track.port)
            .or_default()
            .entry(track.channel)
            .or_default()
            .push(idx);
    }
    out
}

/// 端口字母：port 0→'A'，1→'B'，...，25→'Z'，超限为 '?'。
#[allow(dead_code)] // 当前仅测试辅助使用
fn port_letter(port: u8) -> char {
    if port < 26 {
        (b'A' + port) as char
    } else {
        '?'
    }
}

fn port_root_name(port: u8, _track_count: usize) -> &'static str {
    // 静态字符串池最多覆盖 A-Z（port 0-25），超限回退到 '?'。
    // `_track_count` 预留，未来可在 `TreeItem::Root` 支持动态名称后显示。
    match port {
        0 => "Port A",
        1 => "Port B",
        2 => "Port C",
        3 => "Port D",
        4 => "Port E",
        5 => "Port F",
        6 => "Port G",
        7 => "Port H",
        8 => "Port I",
        9 => "Port J",
        10 => "Port K",
        11 => "Port L",
        12 => "Port M",
        13 => "Port N",
        14 => "Port O",
        15 => "Port P",
        16 => "Port Q",
        17 => "Port R",
        18 => "Port S",
        19 => "Port T",
        20 => "Port U",
        21 => "Port V",
        22 => "Port W",
        23 => "Port X",
        24 => "Port Y",
        25 => "Port Z",
        _ => "Port ?",
    }
}

fn channel_root_name(channel: u8, track_count: usize) -> String {
    format!("Channel {:02} ({})", channel + 1, track_count)
}

fn track_name(track: &Track) -> String {
    if track.name.is_empty() {
        format!("(track #{})", track.id)
    } else {
        track.name.clone()
    }
}

/// 收集指定音轨的 automation lanes 叶子。
#[cfg(test)]
mod tests {
    use super::*;

    fn make_track(id: usize, name: &str, port: u8, channel: u8, is_conductor: bool) -> Track {
        Track {
            id,
            name: name.to_string(),
            port,
            channel,
            display_label: format!("{}{:02}", port_letter(port), channel + 1),
            is_conductor,
            can_delete: !is_conductor,
            is_muted: false,
            is_soloed: false,
            color: None,
        }
    }
    #[test]
    fn port_letter_basic() {
        assert_eq!(port_letter(0), 'A');
        assert_eq!(port_letter(1), 'B');
        assert_eq!(port_letter(25), 'Z');
        assert_eq!(port_letter(26), '?');
    }

    #[test]
    fn group_tracks_orders_and_groups() {
        let tracks = vec![
            make_track(0, "Conductor", 0, 0, true),
            make_track(1, "A0c0", 0, 0, false),
            make_track(2, "A0c1", 0, 1, false),
            make_track(3, "B0c0", 1, 0, false),
            make_track(4, "A0c0_dup", 0, 0, false),
        ];

        let groups = group_tracks_by_port_channel(&tracks);
        assert_eq!(groups.len(), 2);
        let p0 = &groups[&0];
        assert_eq!(p0.len(), 2);
        assert_eq!(p0[&0], vec![1, 4]);
        assert_eq!(p0[&1], vec![2]);
        assert_eq!(groups[&1][&0], vec![3]);
    }

    #[test]
    fn collect_tree_items_has_fixed_entries() {
        let tracks = vec![
            make_track(0, "Conductor", 0, 0, true),
            make_track(1, "Lead", 0, 0, false),
        ];
        let items = collect_tree_items(&tracks, &[]);

        // 2 固定文件叶子 + 1 Conductor 根 + 6 Conductor 子叶子
        assert!(
            items
                .iter()
                .any(|i| matches!(i, TreeItem::Leaf { name, .. } if name == "project.json"))
        );
        assert!(
            items
                .iter()
                .any(|i| matches!(i, TreeItem::Leaf { name, .. } if name == "mapping.json"))
        );
        assert!(
            items
                .iter()
                .any(|i| matches!(i, TreeItem::Root { name, .. } if name == "Conductor"))
        );
        assert!(
            items
                .iter()
                .any(|i| matches!(i, TreeItem::Leaf { name, .. } if name == "Tempo"))
        );

        // Port / Channel / Track
        assert!(
            items
                .iter()
                .any(|i| matches!(i, TreeItem::Root { name, .. } if name == "Port A"))
        );
        assert!(
            items.iter().any(
                |i| matches!(i, TreeItem::Root { name, .. } if name.starts_with("Channel 01"))
            )
        );
        assert!(
            items
                .iter()
                .any(|i| matches!(i, TreeItem::Track { name, .. } if name == "Lead"))
        );

        // Track 下的固定叶子
        assert!(items.iter().any(|i| matches!(i, TreeItem::Leaf { name, item: SelectedItem::Notes { .. }, .. } if name == "Notes")));
        assert!(items.iter().any(|i| matches!(i, TreeItem::Leaf { name, item: SelectedItem::ProgramChange { .. }, .. } if name == "Program Change")));
    }

    #[test]
    fn collect_tree_items_skips_conductor_tracks() {
        let tracks = vec![make_track(0, "Conductor", 0, 0, true)];
        let items = collect_tree_items(&tracks, &[]);
        assert!(!items.iter().any(|i| matches!(i, TreeItem::Track { .. })));
    }
}
