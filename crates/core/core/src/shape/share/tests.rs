//! 形状分享编解码测试（DRAW-034）

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use super::*;
use crate::shape::{
    CopyPathParams, DrawingParams, FunnelParams, LiveShapeParams, ShapeAnchor, ShapeFill,
    ShapeGeometry, ShapeKind, ShapeOp, ShapeRect, Stroke, StrokeKind, StrokeRole, Sym, SymAxis,
    SymParams, TextLayoutMode, TextOp, TextParams, TilesParams,
};

fn line(id: u64) -> ShapeModel {
    let mut shape = ShapeModel::new(id, 1, ShapeKind::Line, 1.0);
    shape.geometry = ShapeGeometry::from_points([(0.0, 60.0), (480.0, 64.0)]);
    shape
}

fn rect_shape(id: u64) -> ShapeModel {
    let mut shape = ShapeModel::new(id, 1, ShapeKind::Custom, 1.0);
    shape.geometry = ShapeGeometry::from_rect(ShapeRect::normalized(0.0, 60.0, 480.0, 72.0));
    shape
}

fn decode_op(line: &str, ppq: u16) -> ShapeModel {
    decode_shapes(line, ppq)
        .expect("应能解码")
        .into_iter()
        .next()
        .expect("至少一个形状")
}

#[test]
fn test_same_ppq_roundtrip() {
    let shape = line(7);
    let text = encode_shapes(std::slice::from_ref(&shape), 480).expect("应能编码");
    assert!(text.starts_with(PREFIX), "必须带本格式前缀");
    assert!(!text.contains('\n'), "分享文本必须是一行");

    let decoded = decode_shapes(&text, 480).expect("应能解码");
    assert_eq!(decoded, vec![shape], "同 PPQ 往返应逐字段一致");
}

#[test]
fn test_cross_ppq_scales_geometry() {
    let shape = line(1);
    let text = encode_shapes(&[shape], 480).expect("应能编码");
    let decoded = decode_op(&text, 960);

    assert_eq!(decoded.geometry.anchors[0].pos.0, 0.0);
    assert_eq!(
        decoded.geometry.anchors[1].pos.0, 960.0,
        "480 tick @480 = 1 beat @960"
    );
    assert_eq!(decoded.geometry.anchors[1].pos.1, 64.0, "key 不随 PPQ 变");
}

#[test]
fn test_cross_ppq_scales_tick_bearing_ops() {
    let mut shape = rect_shape(1);
    let tiles = TilesParams {
        length: 480.0,
        gap: 240.0,
        ..Default::default()
    };
    shape.ops.push(ShapeOp::Tiles(tiles));
    shape.ops.push(ShapeOp::Symmetry(SymParams {
        mode: Sym::Mirror,
        axis: SymAxis::Vertical { x: 480.0 },
        source: 0,
    }));
    shape.ops.push(ShapeOp::Text(TextOp {
        text: "A".into(),
        params: TextParams::default(),
        layout: TextLayoutMode::LegacyBox,
        snap: 480.0,
        merged: false,
        vertical: false,
    }));

    let text = encode_shapes(&[shape], 480).expect("应能编码");
    let decoded = decode_op(&text, 960);

    let ShapeOp::Tiles(tiles) = &decoded.ops[0] else {
        panic!("Tiles 丢失");
    };
    assert_eq!((tiles.length, tiles.gap), (960.0, 480.0));
    let ShapeOp::Symmetry(sym) = &decoded.ops[1] else {
        panic!("Symmetry 丢失");
    };
    assert_eq!(sym.axis, SymAxis::Vertical { x: 960.0 });
    let ShapeOp::Text(text) = &decoded.ops[2] else {
        panic!("Text 丢失");
    };
    assert_eq!(text.snap, 960.0);
}

#[test]
fn test_cross_ppq_scales_funnel_and_copy_along() {
    let mut shape = rect_shape(1);
    shape.ops.push(ShapeOp::Funnel(FunnelParams {
        lines: vec![[[0.0, 60.0], [480.0, 60.0]]],
        wall: [[960.0, 60.0], [960.0, 72.0]],
        starts: Vec::new(),
        fill: ShapeFill::Empty,
        reverse: false,
    }));
    let copy = CopyPathParams {
        pts: vec![[480.0, 60.0]],
        mids: vec![Some([240.0, 64.0])],
        grp: Some([960.0, 60.0]),
        ..Default::default()
    };
    shape.ops.push(ShapeOp::CopyAlong(copy));

    let text = encode_shapes(&[shape], 480).expect("应能编码");
    let decoded = decode_op(&text, 960);

    let ShapeOp::Funnel(funnel) = &decoded.ops[0] else {
        panic!("Funnel 丢失");
    };
    assert_eq!(funnel.lines[0][1][0], 960.0);
    assert_eq!(funnel.wall[0][0], 1920.0);
    let ShapeOp::CopyAlong(copy) = &decoded.ops[1] else {
        panic!("CopyAlong 丢失");
    };
    assert_eq!(copy.pts[0][0], 960.0);
    assert_eq!(copy.mids[0].expect("中点")[0], 480.0);
    assert_eq!(copy.grp.expect("组原点")[0], 1920.0);
}

#[test]
fn test_cross_ppq_scales_live_nested_models() {
    let source = line(2);
    let live = LiveShapeParams {
        strokes: vec![Stroke {
            kind: StrokeKind::Line,
            anchors: vec![ShapeAnchor::new((480.0, 60.0))],
            rect: Some(ShapeRect::normalized(480.0, 60.0, 960.0, 64.0)),
        }],
        src: vec![2],
        from: vec![source],
    };
    let mut shape = rect_shape(1);
    shape.ops.push(ShapeOp::Live(live.clone()));

    let text = encode_shapes(std::slice::from_ref(&shape), 480).expect("应能编码");
    let decoded = decode_op(&text, 960);
    let ShapeOp::Live(decoded_live) = &decoded.ops[0] else {
        panic!("Live 丢失");
    };
    assert_eq!(decoded_live.strokes[0].anchors[0].pos.0, 960.0);
    assert_eq!(decoded_live.strokes[0].rect.expect("框").tick_hi, 1920.0);
    assert_eq!(decoded_live.from[0].geometry.anchors[1].pos.0, 960.0);

    // 反向同 PPQ 往返保留全部字段
    let old = encode_shapes(std::slice::from_ref(&shape), 960).expect("应能编码");
    let roundtrip = decode_op(&old, 960);
    let ShapeOp::Live(roundtrip_live) = &roundtrip.ops[0] else {
        panic!("Live 丢失");
    };
    assert_eq!(roundtrip_live.from[0].geometry.anchors[1].pos.0, 480.0);
}

#[test]
fn test_cross_ppq_scales_drawing_strokes() {
    let mut shape = rect_shape(1);
    shape.ops.push(ShapeOp::Drawing(DrawingParams {
        strokes: vec![Stroke {
            kind: StrokeKind::Line,
            anchors: vec![
                ShapeAnchor::new((480.0, 60.0)),
                ShapeAnchor::new((960.0, 64.0)),
            ],
            rect: Some(ShapeRect::normalized(480.0, 60.0, 960.0, 64.0)),
        }],
        roles: vec![StrokeRole::default()],
    }));

    let text = encode_shapes(std::slice::from_ref(&shape), 480).expect("应能编码");
    let decoded = decode_op(&text, 960);
    let ShapeOp::Drawing(drawing) = &decoded.ops[0] else {
        panic!("Drawing 丢失");
    };
    assert_eq!(drawing.strokes[0].anchors[0].pos.0, 960.0);
    assert_eq!(drawing.strokes[0].anchors[1].pos.0, 1920.0);
    assert_eq!(drawing.strokes[0].rect.expect("框").tick_hi, 1920.0);
    assert_eq!(drawing.roles[0].colour_slot, 0);
}

#[test]
fn test_whitespace_and_newlines_are_ignored() {
    let text = encode_shapes(&[line(1)], 480).expect("应能编码");
    let (prefix, payload) = text.split_at(PREFIX.len());
    let folded =
        format!("{prefix}\n  {}\n", &payload[..payload.len() / 2]) + &payload[payload.len() / 2..];
    let decoded = decode_shapes(&folded, 480).expect("折行文本应能解码");
    assert_eq!(decoded[0].id, 1);
}

#[test]
fn test_prefix_version_and_corruption_errors() {
    assert!(matches!(decode_shapes("", 480), Err(ShareError::Empty)));
    assert!(matches!(
        decode_shapes("SPIDERWEB1:abc", 480),
        Err(ShareError::WrongPrefix)
    ));
    assert!(matches!(
        decode_shapes("LUMINO2:abc", 480),
        Err(ShareError::UnsupportedVersion(version)) if version == "2"
    ));

    let text = encode_shapes(&[line(1)], 480).expect("应能编码");
    let mut truncated = text.clone();
    truncated.pop();
    assert!(decode_shapes(&truncated, 480).is_err(), "截断必须报错");

    let mut altered = text.into_bytes();
    let last = altered.len() - 1;
    altered[last] = if altered[last] == b'A' { b'B' } else { b'A' };
    let altered = String::from_utf8(altered).expect("base64 仍是 ASCII");
    assert!(decode_shapes(&altered, 480).is_err(), "篡改必须报错");
}

#[test]
fn test_too_long_input_is_rejected_before_decode() {
    let text = format!("{PREFIX}{}", "A".repeat(MAX_LINE_LEN));
    assert!(matches!(
        decode_shapes(&text, 480),
        Err(ShareError::TooLong)
    ));
}

#[test]
fn test_invalid_shape_is_rejected_on_encode() {
    let shape = ShapeModel::new(1, 1, ShapeKind::Line, 1.0);
    assert!(matches!(
        encode_shapes(&[shape], 480),
        Err(ShareError::InvalidShape(_))
    ));
}

#[test]
fn test_drawing_kind_is_explicitly_unsupported() {
    let payload = SharePayload {
        app: "test".into(),
        kind: "drawing".into(),
        unit: "beats".into(),
        shapes: vec![line(1)],
    };
    let json = serde_json::to_vec(&payload).expect("JSON");
    let compressed = zstd::stream::encode_all(std::io::Cursor::new(json), 3).expect("压缩");
    let text = format!("{PREFIX}{}", STANDARD.encode(compressed));
    assert!(matches!(
        decode_shapes(&text, 480),
        Err(ShareError::UnsupportedKind(kind)) if kind == "drawing"
    ));
}
