//! 琴键实例构建测试（键位布局驱动的实例产出与按下深度）

use super::*;

#[test]
fn test_build_instances() {
    let mut positions = Vec::new();
    let mut widths = Vec::new();
    let mut last = 0u32;
    update_key_positions(128, &mut last, &mut positions, &mut widths);

    let uniform = MiditrailUniformGpu::default();
    let notes = vec![MiditrailNoteGpu {
        key: 60,
        start_tick: 0,
        end_tick: 1000,
        color_packed: 0xFFFF0000,
        track_idx: 0,
        velocity: 100,
        channel: 0,
        _padding: 0,
    }];
    let active_keys = compute_active_keys(uniform.tick, &notes);
    let press_factors = [0.0f32; 128];
    let mut out = Vec::new();
    build_note_instances(&uniform, &notes, &positions, &widths, &mut out);
    build_key_instances(
        &uniform,
        &active_keys,
        &positions,
        &widths,
        &press_factors,
        &mut out,
    );
    // 128 个键 + 1 个音符
    assert_eq!(out.len(), 129);
}

#[test]
fn test_black_key_press_depth_limited() {
    let mut positions = Vec::new();
    let mut widths = Vec::new();
    let mut last = 0u32;
    update_key_positions(128, &mut last, &mut positions, &mut widths);

    let uniform = MiditrailUniformGpu::default();
    let notes = vec![];
    let active_keys = compute_active_keys(uniform.tick, &notes);
    let press_factors = [0.0f32; 128];
    let mut out = Vec::new();
    build_key_instances(
        &uniform,
        &active_keys,
        &positions,
        &widths,
        &press_factors,
        &mut out,
    );
    assert_eq!(out.len(), 128);

    let white = out
        .iter()
        .find(|i| (i.scale[1] - KEYBOARD_HEIGHT).abs() < 1e-6)
        .expect("应存在白键实例");
    assert!(
        (white.press_depth - KEYBOARD_HEIGHT * 0.5).abs() < 1e-6,
        "白键按下深度应为白键高度的一半"
    );

    let black = out
        .iter()
        .find(|i| (i.scale[1] - BLACK_KEY_HEIGHT).abs() < 1e-6)
        .expect("应存在黑键实例");
    let expected_black_depth = (BLACK_KEY_HEIGHT - KEYBOARD_HEIGHT) * 0.5;
    assert!(
        (black.press_depth - expected_black_depth).abs() < 1e-6,
        "黑键按下深度应最多为高出白键部分的 0.5"
    );
}
