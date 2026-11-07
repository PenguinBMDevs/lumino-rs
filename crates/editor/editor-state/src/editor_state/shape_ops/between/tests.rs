//! Between 组同步测试

use super::*;
use std::collections::BTreeMap;

use lumino_core::shape::{BetweenSet, PatternParams, ShapeGeometry, ShapeKind, TumourParams};

fn line(id: u64) -> ShapeModel {
    let mut model = ShapeModel::new(id, 1, ShapeKind::Line, 1.0);
    model.geometry = ShapeGeometry::from_points([(0.0, 60.0), (100.0, 60.0)]);
    model
}

fn params(id: u64, role: BetweenRole, at: Option<f32>, set: Option<BetweenSet>) -> BetweenParams {
    BetweenParams {
        id,
        role,
        at,
        sig: None,
        push: None,
        vel: None,
        set,
    }
}

#[test]
fn test_sync_generates_steps_between_first_and_last() {
    let mut first = line(1);
    first.ops.push(ShapeOp::Between(params(
        7,
        BetweenRole::First,
        Some(0.0),
        Some(BetweenSet::default()),
    )));
    let mut last = line(2);
    last.ops.push(ShapeOp::Between(params(
        7,
        BetweenRole::Last,
        Some(1.0),
        None,
    )));
    let mut shapes = vec![first, last];
    let warnings = sync_groups(&mut shapes);
    assert!(warnings.is_empty());
    assert_eq!(shapes.len(), 12, "首 + 10 步 + 末");
    assert_eq!(shapes[0].id, 1);
    assert_eq!(shapes[11].id, 2);
    for shape in &shapes[1..11] {
        let between = shape
            .ops
            .iter()
            .find_map(|op| match op {
                ShapeOp::Between(p) => Some(p),
                _ => None,
            })
            .expect("Step 应带 Between");
        assert_eq!(between.role, BetweenRole::Step);
        assert!(between.at.expect("Step 有 at") > 0.0);
        assert!(between.at.expect("Step 有 at") < 1.0);
        assert_eq!(shape.kind, ShapeKind::Line);
    }
    assert_ne!(shapes[1].id, shapes[2].id, "步 id 必须唯一");
}

#[test]
fn test_missing_last_unbinds_group() {
    let mut first = line(1);
    first.ops.push(ShapeOp::Between(params(
        7,
        BetweenRole::First,
        Some(0.0),
        Some(BetweenSet::default()),
    )));
    let mut shapes = vec![first];
    let warnings = sync_groups(&mut shapes);
    assert_eq!(warnings.len(), 1);
    assert!(
        !shapes[0]
            .ops
            .iter()
            .any(|op| matches!(op, ShapeOp::Between(_)))
    );
}

#[test]
fn test_modified_step_downgrades_to_key_and_is_preserved() {
    let mut first = line(1);
    first.ops.push(ShapeOp::Between(params(
        7,
        BetweenRole::First,
        Some(0.0),
        Some(BetweenSet {
            steps: 3,
            ..BetweenSet::default()
        }),
    )));
    let mut last = line(2);
    last.ops.push(ShapeOp::Between(params(
        7,
        BetweenRole::Last,
        Some(1.0),
        None,
    )));
    let mut shapes = vec![first, last];
    assert!(sync_groups(&mut shapes).is_empty());
    assert_eq!(shapes.len(), 5, "首 + 3 步 + 末");

    // 手改中间步：几何变化、sig 保持旧值 ⇒ is_modified = true
    let mut modified = shapes[2].clone();
    modified.geometry.anchors[1].pos.1 += 5.0;
    shapes[2] = modified.clone();

    let warnings = sync_groups(&mut shapes);
    assert!(warnings.is_empty());
    assert_eq!(shapes.len(), 5, "key 保留，其余步补足原步数");
    let key = shapes
        .iter()
        .find(|shape| role_of(shape, 7) == Some(BetweenRole::Key))
        .expect("手改步应降级为 Key");
    assert_eq!(
        key.geometry.anchors[1].pos.1, modified.geometry.anchors[1].pos.1,
        "Key 的几何必须保留，不能被重算覆盖"
    );
    assert_eq!(
        shapes
            .iter()
            .filter(|shape| role_of(shape, 7) == Some(BetweenRole::Step))
            .count(),
        2,
        "Key 占住 0.5 步位后，其余 2 步继续生成"
    );
}

#[test]
fn test_fingerprint_translation_invariant_and_modified() {
    let base = line(1);
    let mut moved = base.clone();
    moved.geometry.anchors[0].pos.0 += 100.0;
    moved.geometry.anchors[1].pos.0 += 100.0;
    assert_eq!(fingerprint(&base), fingerprint(&moved), "平移不应改变指纹");

    let mut changed = base.clone();
    changed.geometry.anchors[1].pos.1 += 4.0;
    assert_ne!(fingerprint(&base), fingerprint(&changed));
    let mut step = base.clone();
    step.ops.push(ShapeOp::Between(params(
        7,
        BetweenRole::Step,
        Some(0.5),
        None,
    )));
    let sig = fingerprint(&step);
    if let Some(ShapeOp::Between(p)) = step.ops.last_mut() {
        p.sig = Some(sig);
    }
    assert!(!is_modified(&step));
    step.geometry.anchors[0].pos.1 += 1.0;
    assert!(is_modified(&step));
}

fn group_with_ops(first_ops: Vec<ShapeOp>, last_ops: Vec<ShapeOp>) -> Vec<ShapeModel> {
    let mut first = line(1);
    first.ops.extend(first_ops);
    first.ops.push(ShapeOp::Between(params(
        7,
        BetweenRole::First,
        Some(0.0),
        Some(BetweenSet {
            steps: 3,
            ..BetweenSet::default()
        }),
    )));
    let mut last = line(2);
    last.ops.extend(last_ops);
    last.ops.push(ShapeOp::Between(params(
        7,
        BetweenRole::Last,
        Some(1.0),
        None,
    )));
    let mut shapes = vec![first, last];
    sync_groups(&mut shapes);
    shapes
}

fn step_at(shapes: &[ShapeModel], at: f32) -> &ShapeModel {
    shapes
        .iter()
        .find(|shape| {
            role_of(shape, 7) == Some(BetweenRole::Step)
                && (super::at_of(shape, 7) - at).abs() < 1e-4
        })
        .expect("应生成对应步")
}

fn tumour_size(shape: &ShapeModel) -> Option<f32> {
    shape.ops.iter().find_map(|op| match op {
        ShapeOp::Tumours(params) => Some(params.size),
        _ => None,
    })
}

fn pattern_height(shape: &ShapeModel) -> Option<f64> {
    shape.ops.iter().find_map(|op| match op {
        ShapeOp::Pattern(params) => Some(
            params
                .vars
                .get("height")
                .copied()
                .unwrap_or(f64::from(params.scale)),
        ),
        _ => None,
    })
}

#[test]
fn test_sync_fades_one_sided_tumours_at_middle() {
    let first_only = group_with_ops(
        vec![ShapeOp::Tumours(TumourParams {
            size: 4.0,
            ..TumourParams::default()
        })],
        Vec::new(),
    );
    assert!(
        (tumour_size(step_at(&first_only, 0.25)).expect("步应保留 tumours") - 2.0).abs() < 1e-3
    );
    assert!(
        tumour_size(step_at(&first_only, 0.5))
            .expect("步应保留 tumours")
            .abs()
            < 1e-3
    );
    assert!(
        tumour_size(step_at(&first_only, 0.75))
            .expect("步应保留 tumours")
            .abs()
            < 1e-3
    );

    let last_only = group_with_ops(
        Vec::new(),
        vec![ShapeOp::Tumours(TumourParams {
            size: 4.0,
            ..TumourParams::default()
        })],
    );
    assert!(
        tumour_size(step_at(&last_only, 0.25))
            .expect("步应保留 tumours")
            .abs()
            < 1e-3
    );
    assert!(
        tumour_size(step_at(&last_only, 0.5))
            .expect("步应保留 tumours")
            .abs()
            < 1e-3
    );
    assert!((tumour_size(step_at(&last_only, 0.75)).expect("步应保留 tumours") - 2.0).abs() < 1e-3);
}

#[test]
fn test_sync_interpolates_two_sided_tumours() {
    let shapes = group_with_ops(
        vec![ShapeOp::Tumours(TumourParams {
            size: 4.0,
            ..TumourParams::default()
        })],
        vec![ShapeOp::Tumours(TumourParams {
            size: 8.0,
            ..TumourParams::default()
        })],
    );
    assert!((tumour_size(step_at(&shapes, 0.25)).expect("步应保留 tumours") - 5.0).abs() < 1e-3);
    assert!((tumour_size(step_at(&shapes, 0.5)).expect("步应保留 tumours") - 6.0).abs() < 1e-3);
    assert!((tumour_size(step_at(&shapes, 0.75)).expect("步应保留 tumours") - 7.0).abs() < 1e-3);
}

#[test]
fn test_sync_fades_one_sided_pattern_at_middle() {
    let pattern = PatternParams {
        vars: BTreeMap::from([("height".to_string(), 4.0)]),
        ..PatternParams::default()
    };
    let first_only = group_with_ops(vec![ShapeOp::Pattern(pattern.clone())], Vec::new());
    assert!(
        (pattern_height(step_at(&first_only, 0.25)).expect("步应保留 pattern") - 2.0).abs() < 1e-3
    );
    assert!(
        pattern_height(step_at(&first_only, 0.5))
            .expect("步应保留 pattern")
            .abs()
            < 1e-3
    );
    assert!(
        pattern_height(step_at(&first_only, 0.75))
            .expect("步应保留 pattern")
            .abs()
            < 1e-3
    );

    let last_only = group_with_ops(Vec::new(), vec![ShapeOp::Pattern(pattern)]);
    assert!(
        pattern_height(step_at(&last_only, 0.25))
            .expect("步应保留 pattern")
            .abs()
            < 1e-3
    );
    assert!(
        pattern_height(step_at(&last_only, 0.5))
            .expect("步应保留 pattern")
            .abs()
            < 1e-3
    );
    assert!(
        (pattern_height(step_at(&last_only, 0.75)).expect("步应保留 pattern") - 2.0).abs() < 1e-3
    );
}

#[test]
fn test_fingerprint_detects_tumour_and_pattern_edits() {
    for op in [
        ShapeOp::Tumours(TumourParams::default()),
        ShapeOp::Pattern(PatternParams::default()),
    ] {
        let mut step = line(1);
        step.ops.push(op);
        let sig = fingerprint(&step);
        step.ops.push(ShapeOp::Between(params(
            7,
            BetweenRole::Step,
            Some(0.5),
            None,
        )));
        if let Some(ShapeOp::Between(between)) = step.ops.last_mut() {
            between.sig = Some(sig);
        }
        assert!(!is_modified(&step), "生成时的指纹应匹配");
        match step.ops.first_mut() {
            Some(ShapeOp::Tumours(tumour)) => tumour.size += 1.0,
            Some(ShapeOp::Pattern(pattern)) => pattern.scale += 1.0,
            _ => unreachable!(),
        }
        assert!(is_modified(&step), "手改 Tumours/Pattern 参数应降级为 Key");
    }
}

#[test]
fn test_sync_clamps_steps_to_most_steps() {
    let mut first = line(1);
    first.ops.push(ShapeOp::Between(params(
        7,
        BetweenRole::First,
        Some(0.0),
        Some(BetweenSet {
            steps: 999,
            ..BetweenSet::default()
        }),
    )));
    let mut last = line(2);
    last.ops.push(ShapeOp::Between(params(
        7,
        BetweenRole::Last,
        Some(1.0),
        None,
    )));
    let mut shapes = vec![first, last];
    let warnings = sync_groups(&mut shapes);
    assert_eq!(shapes.len(), 502, "First + 500 Step + Last");
    assert!(
        warnings
            .iter()
            .any(|warning| warning.message.contains("500")),
        "应报告 steps 钳到 500"
    );
}
