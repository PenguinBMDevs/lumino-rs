//! Gate-aware flip 算法测试
//!
//! 夹具对应上游 `gateflip.py`：同一 key 行上的 4 个门控（r g b w）在自身 run 内镜像。

use super::*;
use crate::ShapeNote;

fn note(tick: f32, key: u16, length: f32, velocity: u8) -> ShapeNote {
    ShapeNote {
        track: 0,
        tick,
        key,
        length,
        velocity,
    }
}

fn colored(tick: f32, key: u16, length: f32, velocity: u8, track: usize) -> ShapeNote {
    ShapeNote {
        track,
        tick,
        key,
        length,
        velocity,
    }
}

fn span(notes: &[ShapeNote]) -> (f32, f32) {
    let start = notes
        .iter()
        .map(|note| note.tick)
        .fold(f32::INFINITY, f32::min);
    let end = notes
        .iter()
        .map(|note| note.tick + note.length)
        .fold(f32::NEG_INFINITY, f32::max);
    (start, end)
}

fn by_time(notes: &[ShapeNote]) -> Vec<ShapeNote> {
    let mut sorted = notes.to_vec();
    sorted.sort_by(|a, b| {
        a.tick
            .partial_cmp(&b.tick)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    sorted
}

#[test]
fn test_four_gates_mirror_rgbw_to_wbgr() {
    let notes = vec![
        colored(0.0, 60, 120.0, 100, 0),
        colored(120.0, 60, 120.0, 90, 1),
        colored(240.0, 60, 120.0, 80, 2),
        colored(360.0, 60, 120.0, 70, 3),
    ];
    let before = span(&notes);
    let out = apply(&notes);

    assert_eq!(
        out.iter().map(|note| note.tick).collect::<Vec<_>>(),
        vec![360.0, 240.0, 120.0, 0.0],
        "输出保持原数组顺序，但起点严格反向"
    );
    assert_eq!(
        out.iter().map(|note| note.track).collect::<Vec<_>>(),
        vec![0, 1, 2, 3],
        "颜色/通道跟随原音符，不重新分配"
    );
    assert_eq!(
        out.iter().map(|note| note.velocity).collect::<Vec<_>>(),
        vec![100, 90, 80, 70],
        "力度跟随原音符"
    );
    assert_eq!(span(&out), before, "run 首起点与末终点不变");

    let sorted = by_time(&out);
    assert_eq!(
        sorted.iter().map(|note| note.track).collect::<Vec<_>>(),
        vec![3, 2, 1, 0],
        "按时间读是 w b g r"
    );
    assert_eq!(
        sorted.iter().map(|note| note.velocity).collect::<Vec<_>>(),
        vec![70, 80, 90, 100],
        "力度序列随门控一起反向"
    );
    assert!(out.iter().all(|note| note.length == 120.0));
}

#[test]
fn test_single_gate_is_unchanged_and_reported() {
    let notes = vec![note(0.0, 60, 120.0, 100)];
    assert_eq!(apply(&notes), notes);
    assert_eq!(
        why_not(&notes, true),
        Some(GateFlipReason::OneGate),
        "单门控行没有可翻内容"
    );
    assert_eq!(
        why_not(&notes, false),
        Some(GateFlipReason::NoGates),
        "非 spam 填充先报 NoGates"
    );
}

#[test]
fn test_runs_with_holes_mirror_independently() {
    let notes = vec![
        note(0.0, 60, 120.0, 100),
        note(120.0, 60, 120.0, 100),
        // 240..600 是空洞
        note(600.0, 60, 120.0, 100),
        note(720.0, 60, 120.0, 100),
        note(840.0, 60, 120.0, 100),
    ];
    let out = apply(&notes);
    assert_eq!(
        out.iter().map(|note| note.tick).collect::<Vec<_>>(),
        vec![120.0, 0.0, 840.0, 720.0, 600.0],
        "两个 run 各自镜像，空洞位置不变"
    );
    assert_eq!(span(&out[0..2]), (0.0, 240.0), "第一个 run 边界不变");
    assert_eq!(span(&out[2..]), (600.0, 960.0), "第二个 run 边界不变");
    assert!(
        out.iter().all(|note| !(240.0..600.0).contains(&note.tick)),
        "空洞里不应出现音符"
    );
}

#[test]
fn test_unequal_lengths_use_interval_reflection() {
    let notes = vec![note(0.0, 60, 100.0, 100), note(100.0, 60, 300.0, 100)];
    let before = span(&notes);
    let out = apply(&notes);
    assert_eq!(out[0].tick, 300.0, "长音的反向位置");
    assert_eq!(out[0].length, 100.0);
    assert_eq!(out[1].tick, 0.0, "短音的反向位置");
    assert_eq!(out[1].length, 300.0);
    assert_eq!(span(&out), before, "首起点/末终点不变");
}

#[test]
fn test_auto_gap_is_mode_plus_half() {
    let notes = vec![
        note(0.0, 60, 100.0, 100),
        note(150.0, 60, 100.0, 100),
        note(300.0, 60, 100.0, 100),
    ];
    assert_eq!(auto_gap(&notes), 75.0, "最常见静音 50 + 25");

    let mut with_hole = notes.clone();
    with_hole.push(note(600.0, 60, 100.0, 100));
    assert_eq!(auto_gap(&with_hole), 75.0, "空洞不应改变最常见的门控间隔");

    assert_eq!(auto_gap(&[note(0.0, 60, 100.0, 100)]), 1.0);
    assert_eq!(
        auto_gap(&[note(0.0, 60, 100.0, 100), note(0.0, 61, 100.0, 100)]),
        1.0,
        "每行只有一条音符时没有 gap"
    );
}

#[test]
fn test_gap_tie_picks_smallest_and_exact_threshold_stays() {
    // gaps: 10、20（并列众数取最小的 10 → 阈值 15）
    let tie = vec![
        note(0.0, 60, 100.0, 100),
        note(110.0, 60, 100.0, 100),
        note(230.0, 60, 100.0, 100),
    ];
    assert_eq!(auto_gap(&tie), 15.0);

    // gaps: 0、1、2；众数 0 → 阈值 1：gap=1 不断，gap=2 断开
    let notes = vec![
        note(0.0, 60, 100.0, 100),
        note(100.0, 60, 100.0, 100),
        note(201.0, 60, 100.0, 100),
        note(303.0, 60, 100.0, 100),
    ];
    let out = apply(&notes);
    assert_eq!(
        out.iter().map(|note| note.tick).collect::<Vec<_>>(),
        vec![201.0, 101.0, 0.0, 303.0],
        "gap=1 留在 run 内，gap=2 另起 run"
    );
}

#[test]
fn test_different_keys_are_separate_rows() {
    let notes = vec![
        note(0.0, 60, 120.0, 100),
        note(120.0, 60, 120.0, 100),
        note(0.0, 62, 120.0, 100),
        note(120.0, 62, 120.0, 100),
    ];
    let out = apply(&notes);
    assert_eq!(
        out.iter().map(|note| note.tick).collect::<Vec<_>>(),
        vec![120.0, 0.0, 120.0, 0.0],
        "不同 key 行各自镜像"
    );
}

#[test]
fn test_why_not_distinguishes_one_gate_and_no_gates() {
    let two_rows_one_note = vec![note(0.0, 60, 120.0, 100), note(0.0, 61, 120.0, 100)];
    assert_eq!(
        why_not(&two_rows_one_note, true),
        Some(GateFlipReason::OneGate),
        "每行只有一条音符"
    );

    let two_on_one_row = vec![note(0.0, 60, 120.0, 100), note(120.0, 60, 120.0, 100)];
    assert_eq!(why_not(&two_on_one_row, true), None, "同一行两条可翻");
    assert_eq!(
        why_not(&two_on_one_row, false),
        Some(GateFlipReason::NoGates)
    );
    assert_eq!(
        why_not(&[], true),
        Some(GateFlipReason::OneGate),
        "空输入没有可翻内容"
    );
}

#[test]
fn test_empty_and_single_input_are_unchanged() {
    assert!(apply(&[]).is_empty());
    let single = vec![note(7.0, 64, 33.0, 88)];
    assert_eq!(apply(&single), single);
}
