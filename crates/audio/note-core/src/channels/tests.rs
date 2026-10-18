//! 通道模式与同键重叠消解测试

use super::*;

fn note(tick: f32, key: u16, length: f32, velocity: u8) -> Note {
    Note::from_raw(tick, key, length, velocity, 0)
}

fn spans(notes: &[Note]) -> Vec<(u16, f32, f32)> {
    notes
        .iter()
        .map(|n| (n.key, n.tick, n.tick + n.length))
        .collect()
}

/// 完整投影（含力度）：`Note` 未实现 `PartialEq`，测试按字段比较
fn dump(notes: &[Note]) -> Vec<(u16, f32, f32, u8)> {
    notes
        .iter()
        .map(|n| (n.key, n.tick, n.tick + n.length, n.velocity))
        .collect()
}

#[test]
fn test_raw_mode_is_noop() {
    let original = vec![
        note(0.0, 60, 100.0, 80),
        note(0.0, 60, 50.0, 120),
        note(40.0, 60, 200.0, 90),
    ];
    let mut notes = original.clone();
    assert_eq!(apply_channel_mode(&mut notes, ChannelMode::Raw), 0);
    assert_eq!(
        dump(&notes),
        dump(&original),
        "As drawn 必须原样保留（允许同键重叠）"
    );
}

#[test]
fn test_single_with_single_note_is_noop() {
    let mut notes = vec![note(0.0, 60, 100.0, 80)];
    assert_eq!(apply_channel_mode(&mut notes, ChannelMode::Single), 0);
    assert_eq!(notes.len(), 1);
}

#[test]
fn test_single_merges_same_tick_loudest_longest() {
    let mut notes = vec![note(0.0, 60, 100.0, 80), note(0.0, 60, 50.0, 120)];
    assert!(apply_channel_mode(&mut notes, ChannelMode::Single) > 0);
    assert_eq!(notes.len(), 1, "同 tick 同键应合并为一条");
    assert_eq!(notes[0].velocity, 120, "应取最响力度");
    assert_eq!(notes[0].length, 100.0, "应取最长长度");
}

#[test]
fn test_single_cuts_earlier_and_stretches_later() {
    let mut notes = vec![note(0.0, 60, 100.0, 80), note(40.0, 60, 200.0, 90)];
    apply_channel_mode(&mut notes, ChannelMode::Single);
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0].length, 40.0, "前者应在后者起点截断");
    assert_eq!(notes[1].length, 200.0, "后者本就更长时保留更长的那端");
    assert_eq!(spans(&notes), vec![(60, 0.0, 40.0), (60, 40.0, 240.0)]);
}

#[test]
fn test_single_stretches_shorter_later_to_previous_end() {
    let mut notes = vec![note(0.0, 60, 100.0, 80), note(40.0, 60, 20.0, 90)];
    apply_channel_mode(&mut notes, ChannelMode::Single);
    assert_eq!(notes[0].length, 40.0, "前者截断到后者起点");
    assert_eq!(notes[1].length, 60.0, "后者延伸到前者原终点（100）");
    assert_eq!(spans(&notes), vec![(60, 0.0, 40.0), (60, 40.0, 100.0)]);
}

#[test]
fn test_non_overlapping_untouched() {
    let original = vec![note(0.0, 60, 100.0, 80), note(200.0, 60, 50.0, 80)];
    let mut notes = original.clone();
    assert_eq!(apply_channel_mode(&mut notes, ChannelMode::Single), 0);
    assert_eq!(
        dump(&notes),
        dump(&original),
        "不重叠的音符一个字节都不该动"
    );
}

#[test]
fn test_adjacent_notes_untouched() {
    let original = vec![note(0.0, 60, 100.0, 80), note(100.0, 60, 50.0, 80)];
    let mut notes = original.clone();
    assert_eq!(
        apply_channel_mode(&mut notes, ChannelMode::Single),
        0,
        "首尾相接（后者起点 = 前者终点）不算重叠"
    );
    assert_eq!(dump(&notes), dump(&original));
}

#[test]
fn test_different_keys_are_independent() {
    let original = vec![note(0.0, 60, 100.0, 80), note(0.0, 62, 50.0, 80)];
    let mut notes = original.clone();
    assert_eq!(
        apply_channel_mode(&mut notes, ChannelMode::Single),
        0,
        "不同音高互不影响"
    );
    assert_eq!(dump(&notes), dump(&original));
}

#[test]
fn test_chain_of_three_tiles_without_overlap() {
    let mut notes = vec![
        note(0.0, 60, 100.0, 80),
        note(40.0, 60, 30.0, 80),
        note(60.0, 60, 10.0, 80),
    ];
    let changed = apply_channel_mode(&mut notes, ChannelMode::Single);
    assert!(changed > 0);
    assert_eq!(
        notes.len(),
        3,
        "链式重叠只截断/延伸，不合并（起点各不相同）"
    );
    assert_eq!(
        spans(&notes),
        vec![(60, 0.0, 40.0), (60, 40.0, 60.0), (60, 60.0, 100.0)],
        "三条应无缝铺满 0..100 且互不重叠"
    );
}

#[test]
fn test_result_is_order_independent() {
    let a = vec![
        note(40.0, 60, 30.0, 80),
        note(0.0, 60, 100.0, 80),
        note(60.0, 60, 10.0, 80),
    ];
    let b = vec![
        note(60.0, 60, 10.0, 80),
        note(0.0, 60, 100.0, 80),
        note(40.0, 60, 30.0, 80),
    ];
    let mut na = a;
    let mut nb = b;
    apply_channel_mode(&mut na, ChannelMode::Single);
    apply_channel_mode(&mut nb, ChannelMode::Single);

    let mut sa = spans(&na);
    let mut sb = spans(&nb);
    sa.sort_by(|x, y| x.partial_cmp(y).expect("可比较"));
    sb.sort_by(|x, y| x.partial_cmp(y).expect("可比较"));
    assert_eq!(sa, sb, "消解结果不得依赖输入顺序");
}

#[test]
fn test_cut_length_clamped_to_minimum() {
    // 极端：后者的起点与前者的终点仅差 0.5 tick → 截断结果抬到下限
    let mut notes = vec![note(0.0, 60, 10.5, 80), note(10.0, 60, 50.0, 80)];
    apply_channel_mode(&mut notes, ChannelMode::Single);
    assert!(
        notes[0].length >= MIN_NOTE_TICKS,
        "截断结果不得短于下限（实际 {}）",
        notes[0].length
    );
}

#[test]
fn test_mode_str_and_parse() {
    assert_eq!(ChannelMode::Raw.as_str(), "raw");
    assert_eq!(ChannelMode::Single.as_str(), "single");
    assert_eq!(ChannelMode::from_str_lossy("single"), ChannelMode::Single);
    assert_eq!(ChannelMode::from_str_lossy("raw"), ChannelMode::Raw);
    assert_eq!(
        ChannelMode::from_str_lossy("future-mode"),
        ChannelMode::Raw,
        "未知模式回退默认（As drawn）"
    );
    assert_eq!(ChannelMode::default(), ChannelMode::Raw);
}
