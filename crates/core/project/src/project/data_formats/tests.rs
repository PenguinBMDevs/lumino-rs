//! 工程专用数据格式测试
//!
//! 从 `data_formats.rs` 拆出（单文件 ≤400 行，含测试不豁免）。

use super::*;
use lumino_core::shape::{
    AnchorBend, AreaSpot, ShapeFill, ShapeGeometry, ShapeKind, ShapeModel, ShapeOp, ShapeRect,
    ShapeStyle, SpamAlign, SpamEnds, TextLayoutMode, TextOp, TextParams,
};

/// 由**改动前**代码生成的 v1 字节夹具（DRAW-040 / D7）。
///
/// 只读、不得重新生成：新代码永远写得对，只有旧字节才能证明「老工程仍能打开」。
const SHAPES_V1_FIXTURE: &[u8] = include_bytes!("../load/fixtures/shapes_v1.lmshapes");

/// 由 **ShapeAnchor bend（v2→v3）改动前** 代码生成的 v2 夹具。
const SHAPES_V2_FIXTURE: &[u8] = include_bytes!("../load/fixtures/shapes_v2.lmshapes");

fn sample_v3() -> LmshapesData {
    let mut shape = ShapeModel::new(7, 1, ShapeKind::Curve, 0.25);
    shape.geometry = ShapeGeometry::from_points([(0.0, 60.0), (480.0, 72.0)]);
    shape.style.align = SpamAlign::Centred;
    shape.style.ends = SpamEnds::Keep;
    shape.style.union = true;
    shape.style.apart = true;
    shape.style.borders = true;
    shape.style.areas = vec![AreaSpot {
        u: 0.25,
        v: 0.75,
        colour: 16,
    }];
    LmshapesData {
        shapes: vec![shape],
    }
}

fn bytes_with_version(magic: &[u8; 4], version: u16) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(6);
    bytes.extend_from_slice(magic);
    bytes.extend_from_slice(&version.to_le_bytes());
    bytes
}

/// D7：改动前生成的 v1 夹具必须能解码，且逐字段与生成时一致
#[test]
fn test_lmshapes_v1_fixture_decodes() {
    let data = LmshapesData::decode(SHAPES_V1_FIXTURE).expect("v1 夹具应能解码");
    assert_eq!(data.shapes.len(), 2, "夹具应含两个形状");

    let first = &data.shapes[0];
    assert_eq!(first.id, 1);
    assert_eq!(first.track, 2);
    assert_eq!(first.kind, ShapeKind::Curve);
    assert_eq!(first.geometry.anchors.len(), 2);
    assert_eq!(first.geometry.anchors[0].pos, (0.0, 60.0));
    assert_eq!(first.geometry.anchors[1].pos, (480.0, 72.0));
    assert!(
        first
            .geometry
            .anchors
            .iter()
            .all(|anchor| anchor.bend == AnchorBend::Smooth),
        "v1 锚点升级后 bend 应为 Smooth"
    );
    assert!(!first.geometry.closed);
    assert_eq!(first.style.fill, ShapeFill::Spam);
    assert!((first.style.gate_beats - 0.125).abs() < f32::EPSILON);
    assert_eq!(first.style.stroke_width, 3);
    assert_eq!(first.style.velocity, 96);
    assert_eq!(first.style.colour_slot, 4);
    assert_eq!(first.ops, vec![ShapeOp::Noop, ShapeOp::Noop]);
    assert!((first.k - 0.25).abs() < f32::EPSILON);
    assert_eq!(first.name, "fixture-1");
    assert!(!first.hidden);

    let second = &data.shapes[1];
    assert_eq!(second.id, 2);
    assert_eq!(second.track, 0);
    assert_eq!(second.kind, ShapeKind::Custom);
    assert!(second.geometry.anchors.is_empty());
    assert_eq!(
        second.geometry.rect,
        Some(ShapeRect {
            tick_lo: 0.0,
            key_lo: 60.0,
            tick_hi: 480.0,
            key_hi: 72.0,
        })
    );
    assert!(second.geometry.closed);
    assert_eq!(second.style, ShapeStyle::default());
    assert!(second.ops.is_empty());
    assert_eq!(second.name, "fixture-2");
    assert!(second.hidden);
}

/// D5：v1 升级时每个 v2 新字段都取显式默认值
#[test]
fn test_lmshapes_v1_upgrade_fills_explicit_defaults() {
    let data = LmshapesData::decode(SHAPES_V1_FIXTURE).expect("v1 夹具应能解码");
    for shape in &data.shapes {
        assert_eq!(shape.style.align, SpamAlign::Auto, "align 默认应为 Auto");
        assert_eq!(shape.style.ends, SpamEnds::Round, "ends 默认应为 Round");
        assert!(!shape.style.union, "union 默认应为 false");
        assert!(!shape.style.apart, "apart 默认应为 false");
        assert!(!shape.style.borders, "borders 默认应为 false");
        assert!(shape.style.areas.is_empty(), "areas 默认应为空");
        assert!(
            shape
                .geometry
                .anchors
                .iter()
                .all(|anchor| anchor.bend == AnchorBend::Smooth),
            "v1 锚点 bend 默认应为 Smooth"
        );
    }
}

/// v2 夹具：顶层锚点与 Live 内嵌锚点都必须能解码，bend 默认 Smooth。
#[test]
fn test_lmshapes_v2_fixture_decodes_with_smooth_bend() {
    let data = LmshapesData::decode(SHAPES_V2_FIXTURE).expect("v2 夹具应能解码");
    assert_eq!(data.shapes.len(), 2, "夹具应含 Curve + Live 两个形状");

    let curve = &data.shapes[0];
    assert_eq!(curve.kind, ShapeKind::Curve);
    assert_eq!(curve.geometry.anchors.len(), 2);
    assert!(
        curve
            .geometry
            .anchors
            .iter()
            .all(|anchor| anchor.bend == AnchorBend::Smooth),
        "v2 顶层锚点 bend 默认 Smooth"
    );
    assert_eq!(curve.style.areas.len(), 1, "v2 areas 应保留");
    assert!(curve.style.borders);

    let live = &data.shapes[1];
    let ShapeOp::Live(params) = &live.ops[0] else {
        panic!("第二个形状应为 Live");
    };
    assert_eq!(params.strokes.len(), 1);
    assert!(
        params.strokes[0]
            .anchors
            .iter()
            .all(|anchor| anchor.bend == AnchorBend::Smooth),
        "v2 Live Stroke 内嵌锚点 bend 默认 Smooth"
    );
    assert_eq!(params.from.len(), 1);
    assert!(
        params.from[0]
            .geometry
            .anchors
            .iter()
            .all(|anchor| anchor.bend == AnchorBend::Smooth),
        "v2 Live from 形状内锚点 bend 默认 Smooth"
    );
}

/// DRAW-029：文字形状走追加的 `ShapeOp::Text`，当前 v3 部件可往返且不改变版本。
#[test]
fn test_lmshapes_text_shape_roundtrip() {
    let mut shape = ShapeModel::new(9, 1, ShapeKind::Custom, 1.0);
    shape.geometry = ShapeGeometry::from_rect(ShapeRect::normalized(0.0, 60.0, 960.0, 72.0));
    shape.name = "text".to_string();
    shape.ops.push(ShapeOp::Text(TextOp {
        text: "LUMINO".to_string(),
        params: TextParams::default(),
        layout: TextLayoutMode::Parametric,
        snap: 480.0,
        merged: false,
        vertical: false,
    }));
    let data = LmshapesData {
        shapes: vec![shape],
    };
    let encoded = data.encode().expect("文字形状编码失败");
    assert_eq!(
        u16::from_le_bytes([encoded[4], encoded[5]]),
        3,
        "追加 ShapeOp 变体不应改变 lmshapes 版本"
    );
    let decoded = LmshapesData::decode(&encoded).expect("文字形状解码失败");
    assert_eq!(decoded.shapes, data.shapes, "文字形状应逐字段往返一致");
}

/// D5：encode 恒写 v3，v3 往返逐字段一致（含 bend / areas）
#[test]
fn test_lmshapes_v3_roundtrip_and_version() {
    let mut data = sample_v3();
    data.shapes[0].geometry.anchors[1].bend = AnchorBend::Corner;
    let encoded = data.encode().expect("编码 v3 失败");
    assert_eq!(&encoded[0..4], LmshapesData::MAGIC);
    assert_eq!(
        u16::from_le_bytes([encoded[4], encoded[5]]),
        3,
        "encode 必须恒写版本 3"
    );

    let decoded = LmshapesData::decode(&encoded).expect("解码 v3 失败");
    assert_eq!(decoded.shapes, data.shapes, "v3 应逐字段往返一致");
}

/// D4：除 lmshapes 外的部件仍只认版本 1（版本 2 必须被拒绝）
#[test]
fn test_other_parts_reject_version_2() {
    assert!(LmtempData::decode(&bytes_with_version(LmtempData::MAGIC, 2)).is_err());
    assert!(LmsigData::decode(&bytes_with_version(LmsigData::MAGIC, 2)).is_err());
    assert!(LmctlData::decode(&bytes_with_version(LmctlData::MAGIC, 2)).is_err());
    assert!(LmtxtData::decode(&bytes_with_version(LmtxtData::MAGIC, 2)).is_err());
    assert!(LmtextmetaData::decode(&bytes_with_version(LmtextmetaData::MAGIC, 2)).is_err());
    assert!(LmsyxData::decode(&bytes_with_version(LmsyxData::MAGIC, 2)).is_err());
    assert!(LmnamesData::decode(&bytes_with_version(LmnamesData::MAGIC, 2)).is_err());
    assert!(LmcatData::decode(&bytes_with_version(LmcatData::MAGIC, 2)).is_err());
}

#[test]
fn test_lmtemp_roundtrip() {
    let data = LmtempData {
        tempo_changes: vec![(0, 120.0), (960, 140.0)],
        default_bpm: 120.0,
    };
    let encoded = data.encode().expect("编码LmtempData失败");
    assert_eq!(&encoded[0..4], LmtempData::MAGIC);
    let decoded = LmtempData::decode(&encoded).expect("解码LmtempData失败");
    assert_eq!(decoded.tempo_changes.len(), 2);
    assert!((decoded.default_bpm - 120.0).abs() < 0.001);
}

#[test]
fn test_lmsig_roundtrip() {
    let data = LmsigData {
        time_signatures: vec![(0, 4, 4), (960, 3, 4)],
        key_signatures: vec![(0, 0, true)],
    };
    let encoded = data.encode().expect("编码LmsigData失败");
    assert_eq!(&encoded[0..4], LmsigData::MAGIC);
    let decoded = LmsigData::decode(&encoded).expect("解码LmsigData失败");
    assert_eq!(decoded.time_signatures.len(), 2);
}

#[test]
fn test_lmctl_roundtrip() {
    let data = LmctlData {
        control_changes: vec![(0, 0, 0, 7, 100)],
        program_changes: vec![(0, 0, 0, 1)],
        // pitch_bends 为相对中心的偏移量（i16；0 = 中心），不是 raw 14-bit。
        pitch_bends: vec![(480, 0, 0, 0)],
    };
    let encoded = data.encode().expect("编码LmctlData失败");
    assert_eq!(&encoded[0..4], LmctlData::MAGIC);
    let decoded = LmctlData::decode(&encoded).expect("解码LmctlData失败");
    assert_eq!(decoded.control_changes.len(), 1);
    assert_eq!(decoded.pitch_bends.len(), 1);
    assert_eq!(decoded.pitch_bends[0].3, 0, "偏移量中心应为 0");
}

#[test]
fn test_lmtxt_roundtrip() {
    let data = LmtxtData {
        lyrics: vec![(0, 0, b"la".to_vec()), (480, 0, b"la".to_vec())],
        markers: vec![(960, 0, b"Chorus".to_vec())],
    };
    let encoded = data.encode().expect("编码LmtxtData失败");
    assert_eq!(&encoded[0..4], LmtxtData::MAGIC);
    let decoded = LmtxtData::decode(&encoded).expect("解码LmtxtData失败");
    assert_eq!(decoded.lyrics.len(), 2);
    assert_eq!(decoded.markers.len(), 1);
    assert_eq!(decoded.lyrics[0].2, b"la");
}

#[test]
fn test_lmtextmeta_roundtrip() {
    let data = LmtextmetaData {
        text_events: vec![
            (0, 0, 0x01, b"hello".to_vec()),
            (120, 1, 0x02, b"(c) test".to_vec()),
            (480, 0, 0x07, b"cue1".to_vec()),
        ],
    };
    let encoded = data.encode().expect("编码LmtextmetaData失败");
    assert_eq!(&encoded[0..4], LmtextmetaData::MAGIC);
    let decoded = LmtextmetaData::decode(&encoded).expect("解码LmtextmetaData失败");
    assert_eq!(decoded.text_events.len(), 3);
    assert_eq!(decoded.text_events[1].2, 0x02);
    assert_eq!(decoded.text_events[2].3, b"cue1");
}

#[test]
fn test_lmsyx_roundtrip() {
    let data = LmsyxData {
        sys_ex: vec![(0, 0, b"\x01\x02\x03\xF7".to_vec())],
    };
    let encoded = data.encode().expect("编码LmsyxData失败");
    assert_eq!(&encoded[0..4], LmsyxData::MAGIC);
    let decoded = LmsyxData::decode(&encoded).expect("解码LmsyxData失败");
    assert_eq!(decoded.sys_ex.len(), 1);
    assert_eq!(decoded.sys_ex[0].2, b"\x01\x02\x03\xF7");
}

#[test]
fn test_lmnames_roundtrip() {
    let data = LmnamesData {
        track_names: vec![Some("Piano".into()), Some("Bass".into()), None],
    };
    let encoded = data.encode().expect("编码LmnamesData失败");
    assert_eq!(&encoded[0..4], LmnamesData::MAGIC);
    let decoded = LmnamesData::decode(&encoded).expect("解码LmnamesData失败");
    assert_eq!(decoded.track_names.len(), 3);
    assert_eq!(decoded.track_names[0], Some("Piano".into()));
    assert_eq!(decoded.track_names[2], None);
}

#[test]
fn test_lmcat_roundtrip() {
    let data = LmcatData {
        channel_aftertouch: vec![(0, 0, 5, 64), (120, 1, 0, 100)],
        poly_aftertouch: vec![(240, 0, 3, 60, 80)],
    };
    let encoded = data.encode().expect("编码LmcatData失败");
    assert_eq!(&encoded[0..4], LmcatData::MAGIC);
    let decoded = LmcatData::decode(&encoded).expect("解码LmcatData失败");
    assert_eq!(decoded.channel_aftertouch.len(), 2);
    assert_eq!(decoded.poly_aftertouch, vec![(240, 0, 3, 60, 80)]);
    assert_eq!(decoded.channel_aftertouch[1].3, 100);
}

#[test]
fn test_invalid_magic() {
    let mut bytes = vec![0u8; 20];
    bytes[0..4].copy_from_slice(b"XXXX");
    assert!(LmtempData::decode(&bytes).is_err());
    assert!(LmsigData::decode(&bytes).is_err());
    assert!(LmctlData::decode(&bytes).is_err());
    assert!(LmtxtData::decode(&bytes).is_err());
    assert!(LmsyxData::decode(&bytes).is_err());
    assert!(LmnamesData::decode(&bytes).is_err());
    assert!(LmcatData::decode(&bytes).is_err());
}
