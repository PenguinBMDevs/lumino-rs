//! Copy along path 音符测试

use super::*;
use lumino_core::shape::{CopyMode, Speed};

fn note() -> ShapeNote {
    ShapeNote {
        track: 0,
        tick: 0.0,
        key: 60,
        length: 10.0,
        velocity: 100,
        channel: 0,
    }
}

#[test]
fn test_runs_start_at_group_origin_not_moving_bbox() {
    let params = CopyPathParams {
        pts: vec![[1000.0, 0.0]],
        grp: Some([0.0, 60.0]),
        ..CopyPathParams::default()
    };
    // 音符包围盒被量化到整数格：base 比 grp 低 1 键也必须不影响路径
    assert_eq!(
        runs(&params, (0.0, 60.0)),
        runs(&params, (0.0, 59.0)),
        "路径起点必须钉在 grp 上，不能跟着包围盒抖动"
    );
    assert_eq!(runs(&params, (0.0, 59.0))[0].a, (0.0, 60.0));
}

#[test]
fn test_copies_never_mirror_when_group_origin_off_note_grid() {
    // 原图占 key 60..61；grp 连续下移时，音符包围盒取整会让 base 与 grp 相差 ±1，
    // 旧实现据此翻转 Dir，把副本上下镜像。
    let notes = vec![
        ShapeNote {
            key: 60,
            length: 200.0,
            ..note()
        },
        ShapeNote {
            key: 61,
            length: 200.0,
            ..note()
        },
    ];
    let mut params = CopyPathParams {
        pts: vec![[480.0, 0.0]],
        mode: CopyMode::Count { n: 1 },
        grp: Some([0.0, 60.0]),
        ..CopyPathParams::default()
    };
    for grp_key in [60.0_f32, 59.9, 59.5, 59.1, 59.0, 58.5] {
        params.grp = Some([0.0, grp_key]);
        let out = copies(&notes, &params, 480.0);
        assert_eq!(out.len(), 2, "grp_key={grp_key}");
        assert!(
            out[0].key <= out[1].key,
            "grp_key={grp_key} 副本被上下镜像：{:?}",
            out.iter().map(|n| n.key).collect::<Vec<_>>()
        );
    }
}

#[test]
fn test_copy_translates_to_corner() {
    let params = CopyPathParams {
        pts: vec![[100.0, 0.0]],
        mode: CopyMode::Count { n: 1 },
        grp: Some([0.0, 60.0]),
        ..CopyPathParams::default()
    };
    let copies = copies(&[note()], &params, 480.0);
    assert_eq!(copies.len(), 1);
    assert!((copies[0].tick - 50.0).abs() < 1e-3);
    assert_eq!(copies[0].key, 60);
}

#[test]
fn test_size_scales_length_and_position() {
    let params = CopyPathParams {
        pts: vec![[100.0, 0.0]],
        mode: CopyMode::Count { n: 2 },
        size: [50.0, 50.0],
        grp: Some([0.0, 60.0]),
        ..CopyPathParams::default()
    };
    let copies = copies(&[note()], &params, 480.0);
    assert_eq!(copies.len(), 2);
    assert!((copies[1].length - 5.0).abs() < 1e-3, "末副本 50% 长度");
}

#[test]
fn test_every_beats_counts_spacing() {
    let params = CopyPathParams {
        pts: vec![[960.0, 0.0]],
        mode: CopyMode::Every { beats: 1.0 },
        speed: Speed::Even,
        grp: Some([0.0, 60.0]),
        ..CopyPathParams::default()
    };
    let copies = copies(&[note()], &params, 480.0);
    assert_eq!(copies.len(), 2, "960 tick / 480 = 2 个副本");
}

#[test]
fn test_copies_of_points_expands_geometry_for_hit_preview() {
    let params = CopyPathParams {
        pts: vec![[100.0, 0.0]],
        mode: CopyMode::Count { n: 1 },
        grp: Some([0.0, 60.0]),
        ..CopyPathParams::default()
    };
    let copies = copies_of_points(&[(0.0, 60.0), (40.0, 60.0)], &params, 480.0);
    assert_eq!(copies.len(), 1);
    assert!((copies[0][0].0 - 50.0).abs() < 1e-3);
    assert!((copies[0][1].0 - 90.0).abs() < 1e-3);
}

#[test]
fn test_channel_shift() {
    let params = CopyPathParams {
        pts: vec![[100.0, 0.0]],
        mode: CopyMode::Count { n: 1 },
        colours: lumino_core::shape::ColourMode::Shift { step: 3 },
        grp: Some([0.0, 60.0]),
        ..CopyPathParams::default()
    };
    let copies = copies(&[note()], &params, 480.0);
    assert_eq!(copies[0].channel, 3);
}
