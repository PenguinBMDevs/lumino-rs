//! 逐音高行填充区间测试：解析交点、even-odd 洞、多环区域身份、背景蔓延

use super::*;

/// 闭环矩形（首尾重复）：tick ∈ [x0, x1]，key ∈ [y0, y1]
fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<(f32, f32)> {
    vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)]
}

/// 带矩形缺口的多边形（缺口 = key 4 以上、tick 4..6 之间挖空）
fn notched() -> Vec<(f32, f32)> {
    vec![
        (0.0, 0.0),
        (10.0, 0.0),
        (10.0, 10.0),
        (4.0, 10.0),
        (4.0, 4.0),
        (6.0, 4.0),
        (6.0, 10.0),
        (0.0, 10.0),
        (0.0, 0.0),
    ]
}

fn regions(keys: &[RegionKey]) -> HashSet<RegionKey> {
    keys.iter().copied().collect()
}

// ── row_spans ──

#[test]
fn test_row_spans_rectangle_is_full_width() {
    // 矩形内部与 key 行相交 = 整条宽度（端点即几何交点，不是格点）
    assert_eq!(row_spans(&rect(0.0, 2.0, 4.0, 5.0), 3.0), vec![(0.0, 4.0)]);
    assert_eq!(row_spans(&rect(0.0, 2.0, 4.0, 5.0), 4.0), vec![(0.0, 4.0)]);
}

#[test]
fn test_row_spans_outside_row_is_empty() {
    assert!(row_spans(&rect(0.0, 2.0, 4.0, 5.0), 9.0).is_empty());
    assert!(row_spans(&rect(0.0, 2.0, 4.0, 5.0), 0.0).is_empty());
}

#[test]
fn test_row_spans_uses_exact_slanted_crossing() {
    // 斜边产生的区间端点必须是**解析交点**（不受任何精度网格约束）。
    // 区间取该行梯形的**最宽跨度**：行内任一高度处的交点都算（Spiderweb
    // row_spans 对每条边按行上下边界各求一次 x，取 min/max）。
    // 三角形 (0,0)-(10,0)-(0,10)：斜边 x = 10 - y
    let tri = vec![(0.0, 0.0), (10.0, 0.0), (0.0, 10.0), (0.0, 0.0)];
    // 行 q=1（y ∈ [0.5, 1.5]）：斜边 x ∈ [8.5, 9.5]
    assert_eq!(row_spans(&tri, 1.0), vec![(0.0, 9.5)]);
    // 行 q=5（y ∈ [4.5, 5.5]）：斜边 x ∈ [4.5, 5.5]
    assert_eq!(row_spans(&tri, 5.0), vec![(0.0, 5.5)]);
    // 区间端点不能是 5.0 这种"行中心"值 —— 那是旧网格采样才会给的答案
}

#[test]
fn test_row_spans_even_odd_keeps_notch_empty() {
    let poly = notched();
    // 缺口上方（key 7）：缺口两侧各一段，中间空
    assert_eq!(row_spans(&poly, 7.0), vec![(0.0, 4.0), (6.0, 10.0)]);
    // 缺口下方（key 2）：整条宽度
    assert_eq!(row_spans(&poly, 2.0), vec![(0.0, 10.0)]);
}

#[test]
fn test_row_spans_degenerate_polygon_is_empty() {
    assert!(row_spans(&[], 3.0).is_empty());
    assert!(row_spans(&[(1.0, 1.0)], 1.0).is_empty());
}

// ── 区域身份 ──

#[test]
fn test_mark_regions_distinguishes_disjoint_and_nested() {
    let outer = rect(0.0, 0.0, 10.0, 10.0);
    let inner = rect(3.0, 3.0, 7.0, 7.0);
    let loops = vec![outer, inner];
    // 外层内部（内环外）→ 只被外环覆盖
    assert_eq!(mark_regions(&loops, &[(1.0, 1)], 1.0), regions(&[0b01]));
    // 内环内部 → 外层 + 内层
    assert_eq!(mark_regions(&loops, &[(5.0, 5)], 1.0), regions(&[0b11]));
    // 全部环外 → 背景
    assert_eq!(mark_regions(&loops, &[(20.0, 20)], 1.0), regions(&[0b0]));
}

#[test]
fn test_mark_regions_two_separate_shapes() {
    let a = rect(0.0, 0.0, 4.0, 4.0);
    let b = rect(10.0, 0.0, 14.0, 4.0);
    let loops = vec![a, b];
    assert_eq!(mark_regions(&loops, &[(2.0, 2)], 1.0), regions(&[0b01]));
    assert_eq!(mark_regions(&loops, &[(12.0, 2)], 1.0), regions(&[0b10]));
    assert_eq!(
        mark_regions(&loops, &[(2.0, 2), (12.0, 2)], 1.0),
        regions(&[0b01, 0b10]),
        "两个标记 = 两个区域"
    );
}

// ── fill_spans ──

#[test]
fn test_fill_spans_outer_only_keeps_hole() {
    let loops = vec![rect(0.0, 0.0, 10.0, 10.0), rect(3.0, 3.0, 7.0, 7.0)];
    let notes = fill_spans(&loops, &regions(&[0b01]), 0.0, 10.0, 5, 5);
    assert_eq!(
        notes,
        vec![
            RawNote {
                start: 0,
                end: 3,
                key: 5
            },
            RawNote {
                start: 7,
                end: 10,
                key: 5
            },
        ],
        "外环填充保留内环的洞"
    );
}

#[test]
fn test_fill_spans_inner_region_fills_only_the_hole() {
    let loops = vec![rect(0.0, 0.0, 10.0, 10.0), rect(3.0, 3.0, 7.0, 7.0)];
    let notes = fill_spans(&loops, &regions(&[0b11]), 0.0, 10.0, 5, 5);
    assert_eq!(
        notes,
        vec![RawNote {
            start: 3,
            end: 7,
            key: 5
        }]
    );
}

#[test]
fn test_fill_spans_background_spreads_outside() {
    let loops = vec![rect(3.0, 0.0, 7.0, 10.0)];
    let notes = fill_spans(&loops, &regions(&[0b0]), 0.0, 10.0, 5, 5);
    assert_eq!(
        notes,
        vec![
            RawNote {
                start: 0,
                end: 3,
                key: 5
            },
            RawNote {
                start: 7,
                end: 10,
                key: 5
            },
        ],
        "背景 = 环外区间"
    );
}

#[test]
fn test_fill_spans_clips_to_tick_range() {
    let loops = vec![rect(0.0, 0.0, 10.0, 10.0)];
    let notes = fill_spans(&loops, &regions(&[0b01]), 4.0, 6.0, 5, 5);
    assert_eq!(
        notes,
        vec![RawNote {
            start: 4,
            end: 6,
            key: 5
        }],
        "钳到范围"
    );
}

#[test]
fn test_fill_spans_covers_every_row_in_range() {
    let loops = vec![rect(0.0, 2.0, 8.0, 5.0)];
    let notes = fill_spans(&loops, &regions(&[0b01]), 0.0, 10.0, 0, 9);
    assert_eq!(notes.len(), 4, "key 2..5 共 4 行各一条");
    assert!(
        notes.iter().all(|n| n.start == 0 && n.end == 8),
        "每行铺满整宽: {notes:?}"
    );
}

#[test]
fn test_fill_spans_no_marks_is_empty() {
    let loops = vec![rect(0.0, 0.0, 10.0, 10.0)];
    assert!(fill_spans(&loops, &HashSet::new(), 0.0, 10.0, 0, 10).is_empty());
}

#[test]
fn test_fill_spans_two_shapes_separate_regions() {
    let a = rect(0.0, 0.0, 4.0, 4.0);
    let b = rect(10.0, 0.0, 14.0, 4.0);
    let loops = vec![a, b];
    let notes = fill_spans(&loops, &regions(&[0b01]), 0.0, 15.0, 2, 2);
    assert_eq!(
        notes,
        vec![RawNote {
            start: 0,
            end: 4,
            key: 2
        }]
    );
    let notes = fill_spans(&loops, &regions(&[0b01, 0b10]), 0.0, 15.0, 2, 2);
    assert_eq!(
        notes,
        vec![
            RawNote {
                start: 0,
                end: 4,
                key: 2
            },
            RawNote {
                start: 10,
                end: 14,
                key: 2
            },
        ],
        "多标记 = 区域并集"
    );
}

#[test]
fn test_merge_spans_joins_touching() {
    assert_eq!(
        merge_spans(&[(6.0, 10.0), (0.0, 6.0)]),
        vec![(0.0, 10.0)],
        "端点相接合成一个"
    );
    assert_eq!(
        merge_spans(&[(0.0, 2.0), (5.0, 7.0)]),
        vec![(0.0, 2.0), (5.0, 7.0)],
        "远离的保持两段"
    );
}
