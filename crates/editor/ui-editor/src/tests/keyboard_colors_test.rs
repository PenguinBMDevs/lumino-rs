//! 键盘颜色功能测试
//!
//! 验证 `update_playback_key_colors` 能否从 MIDI 文档正确读取音符并着色。
//!
//! 2026-09 拆分（原文件 415 行 > 400 行阈值，按场景拆分为同级子模块）：
//! - `basics`：基础着色与边界（二分定位、音符边界、功能开关、无文档、tick 0、越过全部音符）
//! - `clear`：清除行为（停止后清空与幂等）
//! - `incremental`：增量扫描路径与活跃集合不增长
//! - `spatial_index`：跨轨空间索引全量重建路径

mod basics;
mod clear;
mod incremental;
mod spatial_index;

use crate::Editor;
use lumino_midi_loader::{MidiDocument, NoteEvent};

/// 创建一个简单的 MIDI 文档用于测试
pub(crate) fn make_test_doc() -> MidiDocument {
    // 音轨 0：2 个音符（排序前检查排序稳定性）
    let mut track0 = vec![
        NoteEvent::new(480, 960, 64, 100, 0), // E4, 从 tick 480 到 960
        NoteEvent::new(0, 480, 60, 100, 0),   // C4, 从 tick 0 到 480
    ];
    track0.sort_unstable_by_key(|n| n.start_tick);

    // 音轨 1：1 个长音符
    let track1 = vec![
        NoteEvent::new(0, 1920, 67, 100, 1), // G4, 从 tick 0 到 1920
    ];

    let track_count = 2u16;
    MidiDocument {
        notes: vec![
            lumino_midi_loader::ChunkedList::from_sorted(track0),
            lumino_midi_loader::ChunkedList::from_sorted(track1),
        ],
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![(0, 0, false)],
        control_events: lumino_midi_loader::ChunkedList::new(),
        lyrics: vec![],
        markers: vec![],
        text_events: vec![],
        sys_ex: vec![],
        track_names: vec![Some("Track 1".into()), Some("Track 2".into())],
        total_ticks: 1920,
        track_count,
        tracks: lumino_midi_loader::TrackManager::new(track_count),
        division: 480,
        track_ports: vec![],

        track_max_end_ticks: vec![],
    }
}
