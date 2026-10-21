//! Spam 门长落位测试：Align × Ends 与边界

use super::*;

fn span(start: f32, end: f32) -> Span {
    Span { start, end }
}

fn gates(spans: &[Span], align: SpamAlign, ends: SpamEnds, tick0: bool) -> Vec<(f32, f32)> {
    place_gates(spans, 100.0, align, ends, tick0)
        .into_iter()
        .map(|s| (s.start, s.end))
        .collect()
}

#[test]
fn test_auto_round_places_and_caps() {
    assert_eq!(
        gates(&[span(0.0, 250.0)], SpamAlign::Auto, SpamEnds::Round, true),
        vec![(0.0, 100.0), (100.0, 200.0), (200.0, 300.0)],
        "余量 50 = 半门 → 补一个整门"
    );
    assert_eq!(
        gates(&[span(0.0, 50.0)], SpamAlign::Auto, SpamEnds::Round, true),
        vec![(0.0, 100.0)],
        "短于半门也保证至少一个门（half-up 取整）"
    );
}

#[test]
fn test_auto_keep_head_and_tail() {
    assert_eq!(
        gates(&[span(0.0, 250.0)], SpamAlign::Auto, SpamEnds::Keep, true),
        vec![(0.0, 100.0), (100.0, 200.0), (200.0, 250.0)],
        "余量作为短尾音保留"
    );
}

#[test]
fn test_auto_drop_and_short_segment() {
    assert_eq!(
        gates(&[span(0.0, 250.0)], SpamAlign::Auto, SpamEnds::Drop, true),
        vec![(0.0, 100.0), (100.0, 200.0)],
        "丢弃 50 tick 余量"
    );
    assert_eq!(
        gates(&[span(0.0, 50.0)], SpamAlign::Auto, SpamEnds::Drop, true),
        vec![(0.0, 50.0)],
        "整段不足一个门 → 原样一条"
    );
}

#[test]
fn test_min_grows_short_note_to_quarter_gate() {
    assert_eq!(
        gates(&[span(0.0, 10.0)], SpamAlign::Auto, SpamEnds::Min, true),
        vec![(-8.0, 17.0)],
        "10 < 25 → 居中长到 25"
    );
}

#[test]
fn test_stretch_exactly_fills() {
    let got = gates(
        &[span(0.0, 250.0)],
        SpamAlign::Auto,
        SpamEnds::Stretch,
        true,
    );
    assert_eq!(got, vec![(0.0, 83.0), (83.0, 166.0), (166.0, 250.0)]);
    assert_eq!(got.last().expect("应有末门").1, 250.0);
}

#[test]
fn test_centred_round_splits_remainder_both_ends() {
    assert_eq!(
        gates(
            &[span(0.0, 250.0)],
            SpamAlign::Centred,
            SpamEnds::Round,
            true
        ),
        vec![(-25.0, 75.0), (75.0, 175.0), (175.0, 275.0)],
        "余量 50 两端各 25"
    );
}

#[test]
fn test_aligned_round_uses_tick_zero_grid() {
    assert_eq!(
        gates(
            &[span(50.0, 250.0)],
            SpamAlign::Aligned,
            SpamEnds::Round,
            true
        ),
        vec![(0.0, 100.0), (100.0, 200.0), (200.0, 300.0)],
        "至少半个门在段内的网格门"
    );
    assert_eq!(
        gates(
            &[span(120.0, 250.0)],
            SpamAlign::Aligned,
            SpamEnds::Round,
            true
        ),
        vec![(100.0, 200.0), (200.0, 300.0)]
    );
}

#[test]
fn test_aligned_keep_and_drop() {
    assert_eq!(
        gates(
            &[span(50.0, 250.0)],
            SpamAlign::Aligned,
            SpamEnds::Keep,
            true
        ),
        vec![(50.0, 100.0), (100.0, 200.0), (200.0, 250.0)],
        "对齐后保留头部半门与尾部半门"
    );
    assert_eq!(
        gates(
            &[span(50.0, 250.0)],
            SpamAlign::Aligned,
            SpamEnds::Drop,
            true
        ),
        vec![(100.0, 200.0)],
        "Drop 丢弃头部与尾部余量"
    );
}

#[test]
fn test_centred_keep_and_short_segment() {
    assert_eq!(
        gates(
            &[span(0.0, 250.0)],
            SpamAlign::Centred,
            SpamEnds::Keep,
            true
        ),
        vec![(0.0, 25.0), (25.0, 125.0), (125.0, 225.0), (225.0, 250.0)]
    );
    assert_eq!(
        gates(
            &[span(10.0, 60.0)],
            SpamAlign::Centred,
            SpamEnds::Keep,
            true
        ),
        vec![(10.0, 60.0)],
        "短于一个门时整段一条"
    );
}

#[test]
fn test_aligned_relative_origin_when_not_tick0() {
    assert_eq!(
        gates(
            &[span(50.0, 250.0)],
            SpamAlign::Aligned,
            SpamEnds::Round,
            false
        ),
        vec![(50.0, 150.0), (150.0, 250.0)],
        "tick0_aligned=false 时以最左 span 起点为网格原点"
    );
}

#[test]
fn test_invalid_gate_and_empty_spans_are_safe() {
    assert!(place_gates(&[], 100.0, SpamAlign::Auto, SpamEnds::Round, true).is_empty());
    let spans = vec![span(0.0, 100.0)];
    assert_eq!(
        place_gates(&spans, 0.0, SpamAlign::Auto, SpamEnds::Round, true),
        spans,
        "非法 gate 原样返回"
    );
}
