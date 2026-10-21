//! Nested 算法测试

use lumino_core::shape::{NestedParams, NestedWay};

use super::*;

fn area() -> Vec<RowSpan> {
    (60..=64)
        .map(|key| RowSpan {
            key,
            start: 0.0,
            end: 480.0,
        })
        .collect()
}

fn bbox(spans: &[ColouredSpan], colour: u8) -> Option<(f32, f32, u16, u16)> {
    let filtered = spans.iter().filter(|span| span.colour == colour);
    let mut min_tick = f32::INFINITY;
    let mut max_tick = f32::NEG_INFINITY;
    let mut min_key = u16::MAX;
    let mut max_key = 0u16;
    let mut any = false;
    for span in filtered {
        any = true;
        min_tick = min_tick.min(span.start);
        max_tick = max_tick.max(span.end);
        min_key = min_key.min(span.key);
        max_key = max_key.max(span.key);
    }
    any.then_some((min_tick, max_tick, min_key, max_key))
}

#[test]
fn test_scale_three_layers_last_half() {
    let params = NestedParams {
        steps: 3,
        last: 0.5,
        colours: 3,
        ..NestedParams::default()
    };
    let spans = nested_spans(&area(), &params, 1.0);
    let outer = bbox(&spans, 0).expect("第一层");
    let mid = bbox(&spans, 1).expect("第二层");
    let inner = bbox(&spans, 2).expect("第三层");
    assert!((outer.1 - outer.0 - 480.0).abs() < 1e-3);
    assert!((mid.1 - mid.0 - 360.0).abs() < 1e-3);
    assert!((inner.1 - inner.0 - 240.0).abs() < 1e-3, "末层尺寸应为 0.5");
    assert!(outer.0 <= mid.0 && mid.0 <= inner.0, "应逐层向内缩小");
}

#[test]
fn test_skip_leaves_odd_layers_empty() {
    let params = NestedParams {
        steps: 3,
        last: 0.5,
        colours: 3,
        skip: true,
        ..NestedParams::default()
    };
    let spans = nested_spans(&area(), &params, 1.0);
    assert!(bbox(&spans, 0).is_some());
    assert!(bbox(&spans, 1).is_none(), "奇数步应留空");
    assert!(bbox(&spans, 2).is_some());
}

#[test]
fn test_band_has_equal_inset() {
    let params = NestedParams {
        way: NestedWay::Band,
        steps: 3,
        last: 0.5,
        colours: 3,
        ..NestedParams::default()
    };
    let spans = nested_spans(&area(), &params, 1.0);
    let outer = bbox(&spans, 0).expect("第一层");
    let inner = bbox(&spans, 2).expect("第三层");
    assert!((outer.1 - outer.0 - 480.0).abs() < 1e-3);
    assert!(
        (inner.1 - inner.0 - 478.0).abs() < 1e-3,
        "tick 方向每边内缩约 1"
    );
    let outer_h = outer.3 - outer.2;
    let inner_h = inner.3 - inner.2;
    assert_eq!(outer_h - inner_h, 2, "key 方向每边内缩约 1");
}

#[test]
fn test_blend_mix_changes_output_but_outer_edge_stays() {
    let base = NestedParams {
        way: NestedWay::Blend,
        steps: 3,
        last: 0.5,
        mix: 0.0,
        ..NestedParams::default()
    };
    let blended = NestedParams {
        mix: 1.0,
        ..base.clone()
    };
    let a = nested_spans(&area(), &base, 1.0);
    let b = nested_spans(&area(), &blended, 1.0);
    assert_ne!(a, b, "mix=0 与 mix=1 输出应不同");
    let edge_a = bbox(&a, 0).expect("外层");
    let edge_b = bbox(&b, 0).expect("外层");
    assert_eq!(edge_a, edge_b, "形状边缘不应被 Blend 改变");
}
