//! Miditrail 实例构建器单元测试

use super::*;

#[test]
fn test_black_keys() {
    assert!(is_black_key(1));
    assert!(is_black_key(61)); // C#4 + 5 octaves
    assert!(!is_black_key(0));
    assert!(!is_black_key(60));
}

#[test]
fn test_key_positions() {
    let mut positions = Vec::new();
    let mut widths = Vec::new();
    let mut last = 0u32;
    update_key_positions(128, &mut last, &mut positions, &mut widths);
    assert_eq!(positions.len(), 128);
    assert_eq!(widths.len(), 128);
    // 白键总宽度应约为 1.0
    let white_total: f32 = positions
        .iter()
        .enumerate()
        .filter(|(i, _)| !is_black_key(*i as u32))
        .map(|(i, _)| widths[i])
        .sum();
    assert!((white_total - 1.0).abs() < 1e-5);
    // 黑键应比相邻白键窄
    assert!(widths[1] < widths[0]);
}

#[test]
fn test_boost_color_packed_clamps_and_preserves_alpha() {
    // 0xRRGGBBAA 格式：红色 0.5 + 0.5 = 1.0，绿色 0 变 0.5，蓝色 0 变 0.5，alpha 保持不变
    let boosted = boost_color_packed(0x800000FF, 0.5);
    assert_eq!((boosted >> 24) & 0xFF, 0xFF);
    assert_eq!((boosted >> 16) & 0xFF, 0x7F);
    assert_eq!((boosted >> 8) & 0xFF, 0x7F);
    assert_eq!(boosted & 0xFF, 0xFF);
}

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
fn test_build_aura_instances() {
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
    let mut auras = Vec::new();
    build_aura_instances(&uniform, &active_keys, &positions, &widths, &mut auras);
    assert_eq!(auras.len(), 1);
    assert!(auras[0].size > 0.0);
}

#[test]
fn test_note_instances_sorted_by_depth() {
    let mut positions = Vec::new();
    let mut widths = Vec::new();
    let mut last = 0u32;
    update_key_positions(128, &mut last, &mut positions, &mut widths);

    let uniform = MiditrailUniformGpu::default();
    let notes = vec![
        MiditrailNoteGpu {
            key: 60,
            start_tick: 0,
            end_tick: 1000,
            color_packed: 0xFF0000FF,
            track_idx: 0,
            velocity: 100,
            channel: 0,
            _padding: 0,
        },
        MiditrailNoteGpu {
            key: 64,
            start_tick: 1000,
            end_tick: 2000,
            color_packed: 0x00FF00FF,
            track_idx: 0,
            velocity: 100,
            channel: 0,
            _padding: 0,
        },
    ];
    let mut out = Vec::new();
    build_note_instances(&uniform, &notes, &positions, &widths, &mut out);
    assert_eq!(out.len(), 2);
    let front_z = |i: &MiditrailInstanceGpu| i.translation[2] + i.scale[2];
    assert!(
        front_z(&out[0]) <= front_z(&out[1]),
        "音符应按 far-to-near 排序，远的先绘制"
    );
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

#[test]
fn test_note_instances_grouped_by_key_color() {
    let mut positions = Vec::new();
    let mut widths = Vec::new();
    let mut last = 0u32;
    update_key_positions(128, &mut last, &mut positions, &mut widths);

    let uniform = MiditrailUniformGpu::default();
    // 黑键音符更远，白键音符更近；按 Comet 顺序应白键先绘制、黑键后绘制。
    let notes = vec![
        MiditrailNoteGpu {
            key: 61, // C#，黑键
            start_tick: 1000,
            end_tick: 2000,
            color_packed: 0xFFFF0000,
            track_idx: 0,
            velocity: 100,
            channel: 0,
            _padding: 0,
        },
        MiditrailNoteGpu {
            key: 60, // C，白键
            start_tick: 0,
            end_tick: 1000,
            color_packed: 0x00FF0000,
            track_idx: 0,
            velocity: 100,
            channel: 0,
            _padding: 0,
        },
    ];
    let mut out = Vec::new();
    build_note_instances(&uniform, &notes, &positions, &widths, &mut out);
    assert_eq!(out.len(), 2);

    let white_left = positions[60] + widths[60] * 0.04;
    let black_left = positions[61] + widths[61] * 0.04;
    eprintln!("white_left={}, black_left={}", white_left, black_left);
    for (i, inst) in out.iter().enumerate() {
        eprintln!("out[{}].translation[0] = {}", i, inst.translation[0]);
    }
    assert!(
        (out[0].translation[0] - white_left).abs() < 1e-5,
        "白键音符应先绘制"
    );
    assert!(
        (out[1].translation[0] - black_left).abs() < 1e-5,
        "黑键音符应后绘制，覆盖白键音符"
    );
}
