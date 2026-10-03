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

/// 测试用 PPQ
const TEST_PPQ: u16 = 480;

fn regions(keys: &[RegionKey]) -> HashSet<RegionKey> {
    keys.iter().copied().collect()
}

/// 便捷入口：**未开切分档位**（整块填充，切分前的既有语义）
fn fill_all(
    loops: &[Vec<(f32, f32)>],
    regions: &HashSet<RegionKey>,
    tick_lo: f32,
    tick_hi: f32,
    key_lo: i32,
    key_hi: i32,
) -> Vec<RawNote> {
    fill_spans(
        loops, regions, tick_lo, tick_hi, key_lo, key_hi, None, TEST_PPQ,
    )
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
    let notes = fill_all(&loops, &regions(&[0b01]), 0.0, 10.0, 5, 5);
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
    let notes = fill_all(&loops, &regions(&[0b11]), 0.0, 10.0, 5, 5);
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
    let notes = fill_all(&loops, &regions(&[0b0]), 0.0, 10.0, 5, 5);
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
    let notes = fill_all(&loops, &regions(&[0b01]), 4.0, 6.0, 5, 5);
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
    let notes = fill_all(&loops, &regions(&[0b01]), 0.0, 10.0, 0, 9);
    assert_eq!(notes.len(), 4, "key 2..5 共 4 行各一条");
    assert!(
        notes.iter().all(|n| n.start == 0 && n.end == 8),
        "每行铺满整宽: {notes:?}"
    );
}

#[test]
fn test_fill_spans_no_marks_is_empty() {
    let loops = vec![rect(0.0, 0.0, 10.0, 10.0)];
    assert!(fill_all(&loops, &HashSet::new(), 0.0, 10.0, 0, 10).is_empty());
}

#[test]
fn test_fill_spans_two_shapes_separate_regions() {
    let a = rect(0.0, 0.0, 4.0, 4.0);
    let b = rect(10.0, 0.0, 14.0, 4.0);
    let loops = vec![a, b];
    let notes = fill_all(&loops, &regions(&[0b01]), 0.0, 15.0, 2, 2);
    assert_eq!(
        notes,
        vec![RawNote {
            start: 0,
            end: 4,
            key: 2
        }]
    );
    let notes = fill_all(&loops, &regions(&[0b01, 0b10]), 0.0, 15.0, 2, 2);
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

// ── 切分档位（x 分音符） ──

#[test]
fn test_division_step_matches_note_precision() {
    // x 分音符 = 4·ppq/x：四分 = ppq、八分 = ppq/2、十六分 = ppq/4
    assert_eq!(division_step(4, 480), 480.0, "四分音符 = ppq");
    assert_eq!(division_step(8, 480), 240.0, "八分音符 = ppq/2");
    assert_eq!(division_step(16, 480), 120.0, "十六分音符 = ppq/4");
    assert_eq!(division_step(1, 480), 1920.0, "全音符 = 4·ppq");
    // 非 2 的幂也必须可用（用户可填任意数字），且取整到整 tick 防漂移
    assert_eq!(division_step(3, 480), 640.0, "三分音符 = 4·ppq/3");
    assert_eq!(division_step(7, 480), 274.0, "七分音符取整到整 tick");
    // 防御：0 档位（对话框已把 0 归一为 None）不得产生 0/无穷步长
    assert_eq!(division_step(0, 480), 1920.0, "0 退化为全音符档");
}

#[test]
fn test_chop_span_keeps_coverage_and_grid_cuts() {
    // 区间 [0, 480]，步长 120 → 4 条等长音符，切点全部落在网格线上
    assert_eq!(
        chop_span(0.0, 480.0, 120.0),
        vec![(0.0, 120.0), (120.0, 240.0), (240.0, 360.0), (360.0, 480.0)]
    );
    // 非对齐起点：头部残段保留（不丢覆盖），内部切点仍对齐全局网格
    assert_eq!(
        chop_span(30.0, 300.0, 120.0),
        vec![(30.0, 120.0), (120.0, 240.0), (240.0, 300.0)],
        "首尾残段保留 → 总覆盖 = 原区间"
    );
    // 区间短于一步 → 单条
    assert_eq!(chop_span(10.0, 50.0, 120.0), vec![(10.0, 50.0)]);
    // 退化区间
    assert!(chop_span(5.0, 5.0, 120.0).is_empty());
}

#[test]
fn test_fill_spans_division_splits_each_row() {
    // 矩形 tick ∈ [0,480] × key 2..5；十六分音符（120 tick）→ 每行 4 条
    let loops = vec![rect(0.0, 2.0, 480.0, 5.0)];
    let notes = fill_spans(
        &loops,
        &regions(&[0b01]),
        0.0,
        480.0,
        2,
        4,
        Some(16),
        TEST_PPQ,
    );
    assert_eq!(notes.len(), 12, "3 行 × 4 段");
    let row3: Vec<RawNote> = notes.iter().copied().filter(|n| n.key == 3).collect();
    assert_eq!(
        row3,
        vec![
            RawNote {
                start: 0,
                end: 120,
                key: 3
            },
            RawNote {
                start: 120,
                end: 240,
                key: 3
            },
            RawNote {
                start: 240,
                end: 360,
                key: 3
            },
            RawNote {
                start: 360,
                end: 480,
                key: 3
            },
        ],
        "切分后每段 = 一个十六分音符"
    );
}

#[test]
fn test_fill_spans_division_preserves_hole() {
    // 切分不得把内环的洞填上（洞内的原子区间掩码不属于待填区域）
    let loops = vec![rect(0.0, 0.0, 480.0, 10.0), rect(120.0, 3.0, 240.0, 7.0)];
    let notes = fill_spans(
        &loops,
        &regions(&[0b01]),
        0.0,
        480.0,
        5,
        5,
        Some(4),
        TEST_PPQ,
    );
    assert_eq!(
        notes,
        vec![
            RawNote {
                start: 0,
                end: 120,
                key: 5
            },
            RawNote {
                start: 240,
                end: 480,
                key: 5
            },
        ],
        "洞 [120,240] 仍为空，切分只作用于已填区间"
    );
}

#[test]
fn test_fill_spans_no_division_is_single_block() {
    // 未开切分 → 与既有行为完全一致（每个区间一条长音符）
    let loops = vec![rect(0.0, 2.0, 480.0, 5.0)];
    let notes = fill_spans(&loops, &regions(&[0b01]), 0.0, 480.0, 3, 3, None, TEST_PPQ);
    assert_eq!(
        notes,
        vec![RawNote {
            start: 0,
            end: 480,
            key: 3
        }]
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
