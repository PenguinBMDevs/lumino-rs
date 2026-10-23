//! CutRef 簿记测试

use super::*;
use lumino_core::shape::{ShapeGeometry, ShapeKind, ShapeRect};

fn shape() -> ShapeModel {
    let mut shape = ShapeModel::new(1, 1, ShapeKind::Curve, 1.0);
    shape.geometry = ShapeGeometry::from_points([(0.0, 60.0), (120.0, 64.0)]);
    shape
}

#[test]
fn test_outline_keys_tracks_anchors_and_rect() {
    let mut shape = shape();
    assert_eq!(outline_keys(&shape), vec![60, 64]);
    shape.geometry.rect = Some(ShapeRect::normalized(0.0, 55.0, 10.0, 70.0));
    assert_eq!(outline_keys(&shape), vec![60, 64, 55, 70]);
}

#[test]
fn test_cut_ref_invalidates_when_outline_changes() {
    let whole = shape();
    let mut cut = CutRef::new(whole.clone(), (0.0, None));
    cut.knife = Some(KnifeKind::Line);
    cut.marks.push(CutMark {
        id: 1,
        at: 0,
        u: 0.5,
        kind: CutKind::Slice,
        dir: (0.0, 1.0),
    });
    assert!(cut.still_valid(&whole));
    assert!(cut.validate_against(&whole));

    let mut changed = whole.clone();
    changed.geometry.anchors[1].pos.1 += 4.0;
    assert!(!cut.still_valid(&changed));
    assert!(!cut.validate_against(&changed));
    assert!(cut.marks.is_empty(), "失效后切口标记应清除");
    assert!(cut.knife.is_none());
}
