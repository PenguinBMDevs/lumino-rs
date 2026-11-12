//! 角度柔化测试（DRAW-044）

use super::*;

fn params(pairs: &[(usize, f32)]) -> RoundParams {
    let mut params = RoundParams::default();
    for (index, radius) in pairs {
        params.set_radius(*index, *radius);
    }
    params
}

fn dist(a: Pt, b: Pt) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

#[test]
fn test_right_angle_is_replaced_by_tangent_arc() {
    let points = vec![(10.0, 0.0), (0.0, 0.0), (0.0, 10.0)];
    let out = rounded_points(&points, &params(&[(1, 2.0)]), 1.0, 1.0);
    assert_eq!(out.first().copied(), Some((10.0, 0.0)), "端点不变");
    assert_eq!(out.last().copied(), Some((0.0, 10.0)), "端点不变");
    assert!(
        out.iter().any(|p| dist(*p, (2.0, 0.0)) < 1e-6),
        "应有切点 (2,0)：{out:?}"
    );
    assert!(
        out.iter().any(|p| dist(*p, (0.0, 2.0)) < 1e-6),
        "应有切点 (0,2)：{out:?}"
    );
    // 弧上的中间点应到圆心 (2,2) 距离为半径 2
    let centre = (2.0, 2.0);
    let arc_points: Vec<Pt> = out
        .iter()
        .copied()
        .filter(|p| dist(*p, centre) < 2.0 + 1e-6 && dist(*p, centre) > 0.5)
        .collect();
    assert!(arc_points.len() >= 3, "弧应至少采样出 3 个点");
    assert!(
        arc_points
            .iter()
            .all(|p| (dist(*p, centre) - 2.0).abs() < 1e-6),
        "弧点应严格落在半径 2 的圆上"
    );
}

#[test]
fn test_large_radius_is_clamped_to_segment_length() {
    let points = vec![(10.0, 0.0), (0.0, 0.0), (0.0, 10.0)];
    let out = rounded_points(&points, &params(&[(1, 100.0)]), 1.0, 1.0);
    // d 夹到 0.49 * 10 = 4.9；切点仍在段内
    assert!(out.iter().any(|p| dist(*p, (4.9, 0.0)) < 1e-4), "{out:?}");
    assert!(out.iter().any(|p| dist(*p, (0.0, 4.9)) < 1e-4), "{out:?}");
    assert!(
        out.iter().all(|p| p.0 <= 10.0 + 1e-6 && p.1 <= 10.0 + 1e-6),
        "夹取后不应越出相邻段"
    );
}

#[test]
fn test_zero_radius_keeps_original_points() {
    let points = vec![(10.0, 0.0), (0.0, 0.0), (0.0, 10.0)];
    let out = rounded_points(&points, &RoundParams::default(), 1.0, 1.0);
    assert_eq!(out, points);
}

#[test]
fn test_collinear_corner_is_untouched() {
    let points = vec![(10.0, 0.0), (0.0, 0.0), (-10.0, 0.0)];
    let out = rounded_points(&points, &params(&[(1, 2.0)]), 1.0, 1.0);
    assert_eq!(out, points, "折返角不处理");
}

#[test]
fn test_non_finite_radius_is_rejected() {
    let points = vec![(10.0, 0.0), (0.0, 0.0), (0.0, 10.0)];
    for radius in [f32::NAN, f32::INFINITY, -1.0] {
        let out = rounded_points(&points, &params(&[(1, radius)]), 1.0, 1.0);
        assert_eq!(out, points, "非法半径应原样返回");
    }
}

#[test]
fn test_closed_path_rounds_index_zero_and_stays_closed() {
    let points = vec![
        (0.0, 0.0),
        (10.0, 0.0),
        (10.0, 10.0),
        (0.0, 10.0),
        (0.0, 0.0),
    ];
    let out = rounded_points(&points, &params(&[(0, 2.0), (2, 2.0)]), 1.0, 1.0);
    assert!(out.len() > points.len(), "两个角点应被替换成弧");
    assert_eq!(out.first(), out.last(), "闭合路径仍闭合");
    assert!(
        out.iter().all(|p| p.0 >= -1e-6 && p.0 <= 10.0 + 1e-6),
        "弧不应越界"
    );
}

#[test]
fn test_effective_k_makes_short_segments_round() {
    // 形状 480 tick 宽 × 4 key 高。k=0.4167、ppq=480 时 480 tick 折成 2.4 单位
    //（≈2.4 个 key），圆角才不会被 0.49×段长 夹到看不见。
    let points = vec![(0.0, 60.0), (480.0, 60.0), (480.0, 64.0), (0.0, 64.0)];
    let mut params = RoundParams {
        corners: Vec::new(),
        k: 0.4167,
        ppq: 480,
    };
    params.set_radius(1, 2.0);
    let out = rounded_points(&points, &params, 480.0, 0.4167);
    let min_x = out.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let max_x = out.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
    assert!(
        min_x <= 480.0 - 100.0,
        "圆角应明显吃掉一段水平边（当前最小 x = {min_x}）"
    );
    assert!(max_x <= 480.0 + 1e-6, "不应越出形状边界");
}

#[test]
fn test_shallow_v_corner_is_rounded_with_ppq() {
    // V 形：两端同 key、中间节点高 4 key、横向 100 tick。
    // 漏掉 ppq 时 x 被放大 480 倍，夹角算成接近 180°，圆角几乎不可见。
    let points = vec![(0.0, 60.0), (100.0, 64.0), (200.0, 60.0)];
    let mut params = RoundParams {
        corners: Vec::new(),
        k: 0.4167,
        ppq: 480,
    };
    params.set_radius(1, 1.0);
    let out = rounded_points(&points, &params, 480.0, 0.4167);
    let max_key = out.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    assert!(
        max_key < 63.5,
        "V 形顶点应被圆弧削平（当前最高 key = {max_key}）"
    );
}

#[test]
fn test_effective_k_prefers_frozen_value() {
    let params = RoundParams {
        corners: Vec::new(),
        k: 2.0,
        ppq: 480,
    };
    assert!((params.effective_k(1.0) - 2.0).abs() < 1e-6, "优先用冻结值");
    let fallback = RoundParams::default();
    assert!(
        (fallback.effective_k(3.0) - 3.0).abs() < 1e-6,
        "回退到模型 k"
    );
}

#[test]
fn test_effective_ppq_prefers_frozen_value() {
    let params = RoundParams {
        corners: Vec::new(),
        k: 1.0,
        ppq: 480,
    };
    assert_eq!(params.effective_ppq(960), 480, "冻结 PPQ 后不跟随后续改动");
    let fallback = RoundParams::default();
    assert_eq!(fallback.effective_ppq(960), 960, "未冻结时回退到当前 PPQ");
}

#[test]
fn test_anchor_entry_skips_curve_kind() {
    let mut model = lumino_core::shape::ShapeModel::new(0, 1, ShapeKind::Curve, 1.0);
    model.geometry.anchors = vec![
        ShapeAnchor::new((0.0, 0.0)),
        ShapeAnchor::new((10.0, 0.0)),
        ShapeAnchor::new((10.0, 10.0)),
    ];
    let out = rounded_anchors(&model, &params(&[(1, 2.0)]), 1.0);
    assert_eq!(out.len(), model.geometry.anchors.len(), "曲线不做下标柔化");
}
