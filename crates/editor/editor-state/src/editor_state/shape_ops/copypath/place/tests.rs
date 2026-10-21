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
