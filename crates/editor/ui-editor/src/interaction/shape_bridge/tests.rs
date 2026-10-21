//! Shape bridge 测试

use super::*;
use lumino_editor_state::{DrawnShape, DrawnShapeSource, ShapeNote};

fn drawn(source: DrawnShapeSource) -> DrawnShape {
    DrawnShape {
        id: 1,
        model_id: None,
        track: 1,
        group: None,
        source,
        notes: vec![ShapeNote {
            track: 1,
            tick: 0.0,
            key: 60,
            length: 10.0,
            velocity: 100,
            channel: 0,
        }],
        hidden_by_creation: false,
        deleted: false,
        ever_deleted: false,
        moves: Vec::new(),
        pending: None,
    }
}

#[test]
fn test_polyline_roundtrip() {
    let shape = drawn(DrawnShapeSource::Polyline {
        points: vec![(0.0, 60.0), (10.0, 64.0)],
    });
    let model = model_from_drawn(&shape, 1.0);
    assert_eq!(model.kind, ShapeKind::Polyline);
    assert_eq!(model.geometry.anchors.len(), 2);
    match source_from_model(&model) {
        DrawnShapeSource::Polyline { points } => assert_eq!(points.len(), 2),
        _ => panic!("应还原为折线"),
    }
}

#[test]
fn test_shape_geometry_becomes_outline() {
    let shape = drawn(DrawnShapeSource::Shape {
        kind: lumino_editor_state::ShapeKind::Rectangle,
        rect: (0.0, 60.0, 10.0, 64.0),
        shift_constrained: false,
        apex_high: true,
        filled: false,
    });
    let model = model_from_drawn(&shape, 1.0);
    assert!(!model.geometry.anchors.is_empty());
    assert!(matches!(
        source_from_model(&model),
        DrawnShapeSource::Polyline { .. }
    ));
}
