//! CPU 单轨流式会话计划测试（Q3 B）。
//!
//! 无法在无 GPU 环境下 mock `WgpuRenderThread`，因此把「发哪一轨 / 多少实例」
//! 的纯逻辑抽成 [`current_only_plan`] 单测——它就是会话的总长度与轨数来源。

use super::*;
use lumino_midi_model::NoteEvent;

/// 构造测试文档：`notes` 为 `(track, start_tick, end_tick)`。
fn document(track_count: u16, notes: &[(usize, u32, u32)]) -> MidiDocument {
    let mut document = MidiDocument::empty_with_tracks(track_count, 480);
    for &(track, start, end) in notes {
        document.insert_note(track, NoteEvent::new(start, end, 60, 100, 0));
    }
    document
}

/// 只发当前轨：轨号 = `current_track`，长度 = 该轨音符数（其他轨不参与）。
#[test]
fn current_only_plan_targets_single_track() {
    let document = document(3, &[(0, 0, 10), (1, 0, 10), (1, 100, 120), (2, 0, 10)]);
    assert_eq!(
        current_only_plan(Some(&document), 1),
        Some((1, 2)),
        "CPU 单轨会话必须只发当前轨（轨 1 的 2 个音符）"
    );
    // 总长度就是该轨长度：3 轨合计 4 音符，单轨会话只认 2。
    assert_eq!(current_only_plan(Some(&document), 0), Some((0, 1)));
}

/// 空轨也要建段：`Some((track, 0))` 表示「发一个空块占位」，
/// 否则主轨事件级增量（Insert/UpdateMany）在段表里找不到段。
#[test]
fn current_only_plan_keeps_empty_track_segment() {
    let document = document(2, &[(0, 0, 10)]);
    assert_eq!(
        current_only_plan(Some(&document), 1),
        Some((1, 0)),
        "空轨必须返回 Some(0)（发空块占位），不得返回 None"
    );
}

/// 当前轨越界（文档轨数不足）→ 不发 Chunk，段表保持空（安全清空）。
#[test]
fn current_only_plan_out_of_range_is_none() {
    let document = document(2, &[(0, 0, 10)]);
    assert_eq!(current_only_plan(Some(&document), 2), None);
    assert_eq!(current_only_plan(Some(&document), usize::MAX), None);
    // 无文档（空工程）同样不发 Chunk
    assert_eq!(current_only_plan(None, 0), None);
}

/// border_width 编码：低 16 位描边宽、高 16 位 = track_idx + 1。
///
/// CPU 单轨会话依赖这一编码让 shader 认出当前轨（`is_main`）并染主轨蓝。
#[test]
fn border_width_encodes_track_index() {
    for track in [0usize, 1, 7, 65_535] {
        let encoded = onion_border_width(track);
        assert_eq!(encoded & 0xFFFF, super::super::ONION_SKIN_BORDER_WIDTH);
        assert_eq!(
            encoded >> 16,
            (track as u32 + 1) & 0xFFFF,
            "track={track} 的编码必须落在高 16 位"
        );
    }
}
