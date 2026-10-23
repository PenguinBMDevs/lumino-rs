//! Funnel 几何测试

use lumino_core::shape::{FunnelParams, ShapeFill};

use super::*;
use crate::shape_ops::masks::RowSpan;

fn params() -> FunnelParams {
    FunnelParams {
        lines: vec![[[0.0, 20.0], [100.0, 20.0]]],
        wall: [[100.0, 10.0], [100.0, 30.0]],
        starts: Vec::new(),
        fill: ShapeFill::Spam,
        reverse: false,
    }
}

fn close(a: Pt, b: Pt) -> bool {
    (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6
}

#[test]
fn test_slant_box_triangle_relation() {
    let boxes = slant_boxes(&params());
    assert_eq!(boxes.len(), 2, "两个墙端各一个 box");
    for box_ in boxes {
        let cross = (box_.s.0 + box_.u.0, box_.s.1 + box_.u.1);
        assert!(close(cross, (100.0, 20.0)), "S+U 应是线与墙交点");
        let wall_end = (cross.0 + box_.v.0, cross.1 + box_.v.1);
        assert!(
            close(wall_end, (100.0, 10.0)) || close(wall_end, (100.0, 30.0)),
            "S+U+V 应是一个墙端，实际 {wall_end:?}"
        );
    }
}

#[test]
fn test_reverse_keeps_relation() {
    let params = FunnelParams {
        lines: vec![[[200.0, 20.0], [100.0, 20.0]]],
        wall: [[100.0, 10.0], [100.0, 30.0]],
        reverse: true,
        ..params()
    };
    let boxes = slant_boxes(&params);
    assert_eq!(boxes.len(), 2);
    for box_ in boxes {
        assert!(box_.u.0 < 0.0, "反向漏斗 U 应指向更早的墙");
        assert!(close(
            (box_.s.0 + box_.u.0, box_.s.1 + box_.u.1),
            (100.0, 20.0)
        ));
    }
}

#[test]
fn test_rows_are_inside_boxes() {
    let boxes = slant_boxes(&params());
    for box_ in &boxes {
        let rows = funnel_rows(box_, 0.0, 480.0);
        assert!(!rows.is_empty());
        for row in rows {
            for x in [row.start, row.end] {
                let point = (f64::from(x), f64::from(row.key));
                let (u, f) = box_.inverse(point).expect("box 不应退化");
                assert!(
                    (-1e-6..=1.0 + 1e-6).contains(&u) && (-1e-6..=1.0 + 1e-6).contains(&f),
                    "点 {point:?} 反解 ({u}, {f}) 应在 0..=1"
                );
            }
        }
    }
}

#[test]
fn test_wall_end_on_line_produces_one_box() {
    let params = FunnelParams {
        wall: [[100.0, 20.0], [100.0, 30.0]],
        ..params()
    };
    let boxes = slant_boxes(&params);
    assert_eq!(boxes.len(), 1, "落在交点上的墙端不应生成退化 box");
}

#[test]
fn test_gate_halves_double_notes() {
    // 用长线（2000 tick）+ 长行，避免 ceil 余数偏差：行内门数 c 时
    // 「门长减半」的片段数比 = 2 - 1/c，只有 c 足够大才收敛到 2（±5%）。
    let long = FunnelParams {
        lines: vec![[[0.0, 20.0], [2000.0, 20.0]]],
        wall: [[2000.0, 10.0], [2000.0, 30.0]],
        ..params()
    };
    let box_ = slant_boxes(&long)[0];
    let coarse = funnel_rows(&box_, 20.0, 480.0).len();
    let fine = funnel_rows(&box_, 10.0, 480.0).len();
    assert!(coarse > 0);
    let ratio = fine as f64 / coarse as f64;
    assert!(
        (1.9..=2.1).contains(&ratio),
        "门长减半，片段数应翻倍（±5%）：coarse={coarse}, fine={fine}, ratio={ratio:.4}"
    );
}

#[test]
fn test_spam_granularity_rows_are_inside_boxes() {
    // 原卡只验了 gate=0 的整行；这里按 Spam 粒度（gate>0）逐片反解，
    // 断言每一片的起点/中点/终点都落在斜 box 的 (u,f) ∈ [0,1]² 内。
    let boxes = slant_boxes(&params());
    let mut checked = 0usize;
    for box_ in &boxes {
        for row in funnel_rows(box_, 15.0, 480.0) {
            for x in [
                f64::from(row.start),
                (f64::from(row.start) + f64::from(row.end)) * 0.5,
                f64::from(row.end),
            ] {
                let point = (x, f64::from(row.key));
                let (u, f) = box_.inverse(point).expect("box 不应退化");
                assert!(
                    (-1e-6..=1.0 + 1e-6).contains(&u) && (-1e-6..=1.0 + 1e-6).contains(&f),
                    "Spam 片 {point:?} 反解 ({u}, {f}) 落在 box 外"
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 0, "应至少检查到一个 Spam 片");
}

#[test]
fn test_reverse_funnel_rows_are_inside_and_gapless() {
    // 反向漏斗：墙在时间上更早 ⇒ u 指向负方向；区域必须仍然完整覆盖、无重叠。
    let reverse = FunnelParams {
        lines: vec![[[2000.0, 20.0], [1000.0, 20.0]]],
        wall: [[1000.0, 10.0], [1000.0, 30.0]],
        reverse: true,
        ..params()
    };
    let boxes = slant_boxes(&reverse);
    assert_eq!(boxes.len(), 2);
    for box_ in &boxes {
        assert!(box_.u.0 < 0.0, "反向漏斗 U 应指向更早的墙");
        let mut rows_per_key = std::collections::BTreeMap::<u16, Vec<(f32, f32)>>::new();
        for row in funnel_rows(box_, 20.0, 480.0) {
            let point = (f64::from(row.start), f64::from(row.key));
            let (u, f) = box_.inverse(point).expect("box 不应退化");
            assert!(
                (-1e-6..=1.0 + 1e-6).contains(&u) && (-1e-6..=1.0 + 1e-6).contains(&f),
                "反向片起点 {point:?} 反解 ({u}, {f}) 落在 box 外"
            );
            rows_per_key
                .entry(row.key)
                .or_default()
                .push((row.start, row.end));
        }
        // 反向只镜像区域，铺门规则不变：**同一个 box 内**同 key 的门必须背靠背
        // （两个墙端各一个 box，在水平线 + 竖直墙这类退化情形下两者区域会重合，
        //  因此只能在 box 内断言，跨 box 比较会误报）
        for (key, mut spans) in rows_per_key {
            spans.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            for pair in spans.windows(2) {
                assert!(
                    (pair[0].1 - pair[1].0).abs() < 1e-3,
                    "反向漏斗 key={key} 门必须背靠背：{:?} → {:?}",
                    pair[0],
                    pair[1]
                );
            }
        }
    }
}

#[test]
fn test_empty_fill_one_span_per_row_vs_spam_many() {
    // fill=Empty（gate=0）每行一条；fill=Spam（gate>0）每行多条；
    // 两者边界必须一致（首片起点 = 行首、末片终点 = 行尾）。
    let box_ = slant_boxes(&params())[0];
    let empty = funnel_rows(&box_, 0.0, 480.0);
    let spam = funnel_rows(&box_, 15.0, 480.0);
    assert_eq!(
        empty.len(),
        spam.iter()
            .map(|r| r.key)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        "Empty 每行一条"
    );
    assert!(spam.len() > empty.len(), "Spam 应切出更多片");

    let row_bounds = |rows: &[RowSpan]| {
        let mut map = std::collections::BTreeMap::<u16, (f32, f32)>::new();
        for row in rows {
            let entry = map.entry(row.key).or_insert((row.start, row.end));
            entry.0 = entry.0.min(row.start);
            entry.1 = entry.1.max(row.end);
        }
        map
    };
    let empty_bounds = row_bounds(&empty);
    let spam_bounds = row_bounds(&spam);
    assert_eq!(
        empty_bounds.keys().collect::<Vec<_>>(),
        spam_bounds.keys().collect::<Vec<_>>(),
        "两种填充覆盖的 key 行应一致"
    );
    for (key, (start, end)) in &empty_bounds {
        let (spam_start, spam_end) = spam_bounds[key];
        assert!(
            (spam_start - start).abs() < 1e-3,
            "key={key} 首片起点应一致"
        );
        assert!((spam_end - end).abs() < 1e-3, "key={key} 末片终点应一致");
    }
}

#[test]
fn test_box_outline_is_closed() {
    let box_ = slant_boxes(&params())[0];
    let outline = box_outline(&box_);
    assert_eq!(outline.first(), outline.last());
    assert_eq!(outline.len(), 5);
}
