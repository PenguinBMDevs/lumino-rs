//! 算子链与 Tumours 几何测试

use lumino_core::shape::{
    CycleParams, GlueParams, MaskParams, ShapeOp, SustainParams, TumourParams, TumourShape,
    TumourSide,
};

use super::*;
use crate::ShapeNote;

const PPQ: f64 = 480.0;

fn line(length: f64) -> Vec<(f64, f64)> {
    vec![(0.0, 60.0), (length, 60.0)]
}

fn params() -> TumourParams {
    TumourParams {
        k: 1.0,
        dist: 1.0,
        length: 0.5,
        size: 4.0,
        side: TumourSide::Left,
        ..TumourParams::default()
    }
}

#[test]
fn test_noop_returns_same_path() {
    let path = line(960.0);
    let out = apply_geometry_ops(&path, &[ShapeOp::Noop], &OpContext::new(PPQ));
    assert_eq!(out, path);
}

#[test]
fn test_tumours_off_returns_same_path() {
    let path = line(960.0);
    let p = TumourParams {
        on: false,
        ..params()
    };
    let (out, report) = apply_tumours(&path, &p, PPQ);
    assert_eq!(out, path);
    assert_eq!(report.bumps, 0);
}

#[test]
fn test_triangle_bump_goes_left_of_path() {
    let path = line(480.0);
    let p = params();
    let (out, report) = apply_tumours(&path, &p, PPQ);
    assert_eq!(report.bumps, 1, "0..480 内应只有一个凸起起点");
    assert!(
        out.iter().any(|point| point.1 > 60.5),
        "Left 侧凸起应高于主路径"
    );
    assert!(
        out.iter().all(|point| point.1 >= 60.0 - 1e-6),
        "Left 侧不应低于主路径"
    );
    assert!(
        out.iter()
            .all(|point| point.0.is_finite() && point.1.is_finite())
    );
}

#[test]
fn test_side_right_flips_direction() {
    let p = TumourParams {
        side: TumourSide::Right,
        ..params()
    };
    let (out, _) = apply_tumours(&line(480.0), &p, PPQ);
    assert!(
        out.iter().any(|point| point.1 < 59.5),
        "Right 侧应低于主路径"
    );
    assert!(out.iter().all(|point| point.1 <= 60.0 + 1e-6));
}

#[test]
fn test_side_alt_alternates() {
    let p = TumourParams {
        side: TumourSide::Alt,
        dist: 0.5,
        length: 0.25,
        ..params()
    };
    let (out, report) = apply_tumours(&line(960.0), &p, PPQ);
    assert!(report.bumps >= 3, "应生成多个凸起");
    assert!(out.iter().any(|point| point.1 > 60.5), "应有向上凸起");
    assert!(out.iter().any(|point| point.1 < 59.5), "应有向下凸起");
}

#[test]
fn test_side_random_is_deterministic() {
    let p = TumourParams {
        side: TumourSide::Random,
        seed: 7,
        dist: 0.25,
        length: 0.2,
        ..params()
    };
    let first = apply_tumours(&line(960.0), &p, PPQ);
    let second = apply_tumours(&line(960.0), &p, PPQ);
    assert_eq!(first, second, "同一 seed 必须产生同一结果");
    assert!(first.1.bumps >= 4);
}

#[test]
fn test_length_zero_makes_spike() {
    let p = TumourParams {
        length: 0.0,
        ..params()
    };
    let (out, report) = apply_tumours(&line(480.0), &p, PPQ);
    assert_eq!(report.bumps, 1);
    let has_apex = out.iter().any(|point| point.1 > 60.5);
    assert!(has_apex, "length=0 应产生尖刺（顶点离开主路径）");
    assert!(out.iter().any(|point| (point.1 - 60.0).abs() < 1e-6));
}

#[test]
fn test_range_keeps_start_segment_flat() {
    let p = TumourParams {
        start: 0.5,
        end: 1.0,
        ..params()
    };
    let (out, report) = apply_tumours(&line(960.0), &p, PPQ);
    assert_eq!(report.bumps, 1, "区间 0.5..1.0 内只有一个起点");
    for point in &out {
        if point.0 < 480.0 - 1e-6 {
            assert!(
                (point.1 - 60.0).abs() < 1e-6,
                "start 之前不应有凸起：{point:?}"
            );
        }
    }
}

#[test]
fn test_fit_rounds_to_integer_bumps() {
    let p = TumourParams {
        dist: 0.6,
        length: 0.2,
        fit: true,
        ..params()
    };
    let (_, report) = apply_tumours(&line(960.0), &p, PPQ);
    assert_eq!(report.bumps, 3, "960 tick / 0.6 拍应取整为 3 个凸起");
    assert!(!report.truncated);
}

#[test]
fn test_overlap_is_cut_at_next_start() {
    let p = TumourParams {
        dist: 0.5,
        length: 2.0,
        ..params()
    };
    let (out, report) = apply_tumours(&line(960.0), &p, PPQ);
    assert_eq!(report.bumps, 4, "0/240/480/720 四个起点");
    assert!(
        out.iter()
            .all(|point| point.0 >= -1e-6 && point.0 <= 960.0 + 1e-6)
    );
}

#[test]
fn test_closed_loop_does_not_panic_and_stays_finite() {
    let path = vec![(0.0, 60.0), (480.0, 72.0), (0.0, 60.0)];
    let (out, report) = apply_tumours(&path, &params(), PPQ);
    assert!(report.bumps >= 1);
    assert!(
        out.iter()
            .all(|point| point.0.is_finite() && point.1.is_finite())
    );
    assert!(out.len() > path.len());
}

#[test]
fn test_max_tumours_truncates() {
    let p = TumourParams {
        dist: 0.01,
        length: 0.005,
        ..params()
    };
    let (_, report) = apply_tumours(&line(1_000_000.0), &p, PPQ);
    assert_eq!(report.bumps, tumours::MAX_TUMOURS);
    assert!(report.truncated, "超出上限必须报告截断");
}

#[test]
fn test_note_ops_are_noop_for_geometry_op() {
    let mut notes = vec![ShapeNote {
        track: 0,
        tick: 0.0,
        key: 60,
        length: 120.0,
        velocity: 100,
        channel: 0,
    }];
    let ops = vec![ShapeOp::Tumours(params())];
    apply_note_ops(&mut notes, &ops, &NoteOpContext::default());
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].key, 60);
}

#[test]
fn test_note_ops_applies_gate_flip() {
    let mut notes = vec![
        ShapeNote {
            track: 0,
            tick: 0.0,
            key: 60,
            length: 120.0,
            velocity: 100,
            channel: 0,
        },
        ShapeNote {
            track: 0,
            tick: 120.0,
            key: 60,
            length: 120.0,
            velocity: 100,
            channel: 0,
        },
        ShapeNote {
            track: 0,
            tick: 240.0,
            key: 60,
            length: 120.0,
            velocity: 100,
            channel: 0,
        },
    ];
    let ops = vec![ShapeOp::GateFlip];
    apply_note_ops(&mut notes, &ops, &NoteOpContext::default());
    assert_eq!(
        notes.iter().map(|note| note.tick).collect::<Vec<_>>(),
        vec![240.0, 120.0, 0.0],
        "GateFlip 应通过算子链生效"
    );
}

#[test]
fn test_note_ops_applies_cycle_channels() {
    let mut notes = vec![
        ShapeNote {
            track: 0,
            tick: 0.0,
            key: 60,
            length: 100.0,
            velocity: 100,
            channel: 0,
        },
        ShapeNote {
            track: 0,
            tick: 120.0,
            key: 60,
            length: 100.0,
            velocity: 100,
            channel: 0,
        },
        ShapeNote {
            track: 0,
            tick: 240.0,
            key: 60,
            length: 100.0,
            velocity: 100,
            channel: 0,
        },
    ];
    let ops = vec![ShapeOp::Cycle(CycleParams {
        n: 3,
        ..CycleParams::default()
    })];
    apply_note_ops(&mut notes, &ops, &NoteOpContext::default());
    assert_eq!(
        notes.iter().map(|n| n.channel).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
}

#[test]
fn test_note_ops_ignore_mask_marker() {
    let mut notes = vec![ShapeNote {
        track: 0,
        tick: 0.0,
        key: 60,
        length: 120.0,
        velocity: 100,
        channel: 0,
    }];
    let before = notes.clone();
    let ops = vec![ShapeOp::Mask(MaskParams::default())];
    apply_note_ops(&mut notes, &ops, &NoteOpContext::default());
    assert_eq!(notes, before, "Mask 是标记，不应直接改音符");
}

#[test]
fn test_note_ops_applies_sustain() {
    let mut notes = vec![
        ShapeNote {
            track: 0,
            tick: 0.0,
            key: 60,
            length: 100.0,
            velocity: 100,
            channel: 0,
        },
        ShapeNote {
            track: 0,
            tick: 50.0,
            key: 60,
            length: 100.0,
            velocity: 100,
            channel: 0,
        },
    ];
    let ops = vec![ShapeOp::Sustain(SustainParams::default())];
    apply_note_ops(
        &mut notes,
        &ops,
        &NoteOpContext {
            ppq: PPQ,
            ..NoteOpContext::default()
        },
    );
    assert_eq!(notes[0].length, 150.0, "组末 150；Sustain 应通过算子链生效");
    assert_eq!(notes[1].length, 100.0);
}

#[test]
fn test_note_ops_applies_glue() {
    let mut notes = vec![
        ShapeNote {
            track: 0,
            tick: 0.0,
            key: 60,
            length: 10.0,
            velocity: 100,
            channel: 0,
        },
        ShapeNote {
            track: 0,
            tick: 10.0,
            key: 60,
            length: 10.0,
            velocity: 100,
            channel: 0,
        },
    ];
    let ops = vec![ShapeOp::Glue(GlueParams::All)];
    apply_note_ops(&mut notes, &ops, &NoteOpContext::default());
    assert_eq!(notes.len(), 1, "Glue 应合并相接音符");
    assert_eq!(notes[0].tick, 0.0);
    assert_eq!(notes[0].length, 20.0);
}

#[test]
fn test_four_shapes_produce_different_profiles() {
    let mut counts = Vec::new();
    for shape in [
        TumourShape::Triangle,
        TumourShape::Square,
        TumourShape::Circle,
        TumourShape::Parabola,
    ] {
        let p = TumourParams { shape, ..params() };
        let (out, _) = apply_tumours(&line(480.0), &p, PPQ);
        counts.push(out.len());
        assert!(out.iter().any(|point| point.1 > 60.5), "{shape:?} 应有凸起");
    }
    assert!(
        counts.iter().all(|&count| count > 2),
        "四种形态都应产出几何：{counts:?}"
    );
}
