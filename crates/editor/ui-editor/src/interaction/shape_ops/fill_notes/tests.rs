//! Gate Range 在生成链路上的行为测试（DRAW-013 / DRAW-043）
//!
//! 原来 `rows` / `join` / 短段首门 三条只有实现、只被 bincode 往返覆盖，
//! 这里直接对 `time_row_spans` / `time_range_spans` 这两个消费函数下断言。

use lumino_core::shape::GateRangeParams;
use lumino_editor_state::shape_ops::gate_range::range_squares;

use super::{Span, time_range_spans, time_row_spans};

const GATE0: i64 = 30;
const GATE1: i64 = 120;

fn straight() -> Vec<[f32; 2]> {
    vec![[0.0, 0.0], [1.0, 1.0]]
}

fn params(rows: bool, join: bool) -> GateRangeParams {
    GateRangeParams {
        rows,
        join,
        ..GateRangeParams::default()
    }
}

fn max_length(spans: &[Span]) -> f32 {
    spans
        .iter()
        .map(|span| span.end - span.start)
        .fold(0.0_f32, f32::max)
}

fn covers(spans: &[Span], tick: f32) -> bool {
    spans
        .iter()
        .any(|span| span.start <= tick && tick < span.end)
}

#[test]
fn test_rows_true_runs_full_graph_on_each_row() {
    // 单独一行覆盖 [0,480)：rows=true 时该行自己跑完整条 graph，
    // 行末门长应接近 gate1（120），而不是全局归一后的中间值。
    let spans = vec![Span {
        start: 0.0,
        end: 480.0,
    }];
    let out = time_row_spans(&spans, &params(true, false), GATE0, GATE1, &straight());
    assert!(!out.is_empty());
    assert_eq!(out[0].start, 0.0, "首片应贴在行首");
    assert_eq!(
        out.last().map(|span| span.end),
        Some(480.0),
        "末片应收到行末"
    );
    // 最后一道门必然被行末裁剪（480 不是各门长的整数倍），所以取「接近 gate1」
    // 的稳健下界：rows=true 实测 100；若退化成全局归一（rows=false）只有 ~72。
    assert!(
        max_length(&out) >= 90.0,
        "rows=true 该行应走到接近 gate1=120 的大门（受行末裁剪，下界 90），实际最大门长 {}",
        max_length(&out)
    );
}

#[test]
fn test_rows_false_compresses_graph_over_shape_wide_time() {
    // 同一段 [0,480)，但 rows=false 走「全局门序列」（按整形状 [0,960) 归一），
    // 该行只落到 graph 的前半段 → 门长明显更小。这正是不开 rows 的代价。
    let global = range_squares(0, 960, GATE0, GATE1, &straight(), false);
    let spans = vec![Span {
        start: 0.0,
        end: 480.0,
    }];
    let compressed = time_range_spans(&spans, &global, &GateRangeParams::default());
    let per_row = time_row_spans(&spans, &params(true, false), GATE0, GATE1, &straight());
    assert!(
        max_length(&per_row) > max_length(&compressed) + 20.0,
        "rows=true 应比全局归一给到更大的门长：per_row={} compressed={}",
        max_length(&per_row),
        max_length(&compressed)
    );
}

#[test]
fn test_join_true_treats_hole_as_one_segment() {
    // 同一行两段 [0,100) 与 [200,300)，中间空洞 100..200
    let spans = vec![
        Span {
            start: 0.0,
            end: 100.0,
        },
        Span {
            start: 200.0,
            end: 300.0,
        },
    ];
    let separate = time_row_spans(&spans, &params(true, false), GATE0, GATE1, &straight());
    let joined = time_row_spans(&spans, &params(true, true), GATE0, GATE1, &straight());

    assert!(covers(&separate, 50.0), "join=false 仍要填第一段");
    assert!(
        !covers(&separate, 150.0),
        "join=false 时空洞 100..200 必须保持为空"
    );
    assert!(covers(&joined, 50.0), "join=true 第一段照填");
    assert!(
        covers(&joined, 150.0),
        "join=true 应把首段到末段算一整段，空洞被铺满"
    );
}

#[test]
fn test_short_segment_yields_exactly_one_note() {
    // 段长 5 << 门长 30：必须恰好产出一条、覆盖整段，不许 0 条（消失）也不许 2 条（溢出）。
    // 口径说明：实现取「段长」而非上游的「首门长」——首门长会越过段边界，
    // 与 fit / 裁剪口径冲突；本用例锁定「恰好一条且不溢出」。
    let spans = vec![Span {
        start: 0.0,
        end: 5.0,
    }];
    let out = time_row_spans(&spans, &params(true, false), GATE0, GATE1, &straight());
    assert_eq!(out.len(), 1, "短段必须恰好一条音符，实际 {} 条", out.len());
    assert_eq!(out[0].start, 0.0);
    assert_eq!(out[0].end, 5.0, "不得溢出段边界");
}

#[test]
fn test_rows_pieces_stay_back_to_back_without_gap() {
    // 生成侧的不变式：同一条 span 内铺出的门必须首尾相接，不留缝隙也不重叠
    let spans = vec![Span {
        start: 0.0,
        end: 480.0,
    }];
    let out = time_row_spans(&spans, &params(true, false), GATE0, GATE1, &straight());
    for pair in out.windows(2) {
        assert!(
            (pair[0].end - pair[1].start).abs() < 1e-3,
            "门必须背靠背：{:?} → {:?}",
            pair[0],
            pair[1]
        );
    }
}
