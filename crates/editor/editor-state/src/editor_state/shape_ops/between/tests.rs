//! Between 组同步测试

use super::*;
use lumino_core::shape::{BetweenSet, ShapeGeometry, ShapeKind};

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
