//! Funnel 几何测试

use lumino_core::shape::{FunnelParams, ShapeFill};

use super::*;

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
    let box_ = slant_boxes(&params())[0];
    let coarse = funnel_rows(&box_, 20.0, 480.0).len();
    let fine = funnel_rows(&box_, 10.0, 480.0).len();
    assert!(coarse > 0);
    assert!(
        fine as f32 >= coarse as f32 * 1.8,
        "门长减半，片段数应约翻倍（coarse={coarse}, fine={fine}）"
    );
}

#[test]
fn test_box_outline_is_closed() {
    let box_ = slant_boxes(&params())[0];
    let outline = box_outline(&box_);
    assert_eq!(outline.first(), outline.last());
    assert_eq!(outline.len(), 5);
}
