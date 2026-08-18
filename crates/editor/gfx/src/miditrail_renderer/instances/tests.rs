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
    build_aura_instances(
        &uniform,
        &notes,
        &active_keys,
        &positions,
        &widths,
        &mut auras,
    );
    assert_eq!(auras.len(), 1);
    assert!(auras[0].size > 0.0);
    // 光环位于键中心
    let width = widths[60];
    assert!((auras[0].pos - (positions[60] + width * 0.5)).abs() < 1e-5);
}

/// 按下闪光：tick == start 时光环最大（常态 0.5 + 闪光峰值 100/600），
/// 随帧数二次衰减，10 帧后闪光归零回到常态尺寸。
#[test]
fn test_aura_flash_decays_after_press() {
    let mut positions = Vec::new();
    let mut widths = Vec::new();
    let mut last = 0u32;
    update_key_positions(128, &mut last, &mut positions, &mut widths);

    // ppq=480 @ 120 BPM → 960 ticks/s；60fps → 每帧 16 tick
    let uniform = MiditrailUniformGpu {
        tick: 0,
        ppq: 480,
        ticks_per_second: 960.0,
        fps: 60.0,
        ..MiditrailUniformGpu::default()
    };
    let notes = vec![MiditrailNoteGpu {
        key: 60,
        start_tick: 0,
        end_tick: 480 * 4,
        color_packed: 0xFFFF0000,
        track_idx: 0,
        velocity: 100,
        channel: 0,
        _padding: 0,
    }];

    let aura_size_at = |tick: u32| {
        let uniform = MiditrailUniformGpu { tick, ..uniform };
        let active_keys = compute_active_keys(tick, &notes);
        let mut auras = Vec::new();
        build_aura_instances(
            &uniform,
            &notes,
            &active_keys,
            &positions,
            &widths,
            &mut auras,
        );
        auras[0].size
    };

    let width = widths[60];
    // 按下瞬间：AURA_RING_SCALE × 键宽 × (0.5 + 100/600) = 系数 × 键宽 × 2/3
    let peak = width * AURA_RING_SCALE * (0.5 + 100.0 / 600.0);
    assert!((aura_size_at(0) - peak).abs() < 1e-4, "按下瞬间应达到最大");
    // 1 帧后（16 tick）：闪光 = (10-1)²/600 = 0.135
    let f1 = width * AURA_RING_SCALE * (0.5 + 81.0 / 600.0);
    assert!((aura_size_at(16) - f1).abs() < 1e-4, "第 1 帧闪光应衰减");
    // 10 帧后（160 tick）：闪光归零，回到常态 AURA_RING_SCALE × 键宽 × 0.5
    let held = width * AURA_RING_SCALE * 0.5;
    assert!(
        (aura_size_at(160) - held).abs() < 1e-4,
        "10 帧后应回到常态尺寸"
    );
    // 保持期（剩余时长 > 1s，tick < 1920-960）：仍为常态
    assert!(
        (aura_size_at(800) - held).abs() < 1e-4,
        "保持期内应维持常态尺寸"
    );
}

/// 临近结束收缩：最后 1 秒内光环按 (剩余/时长)^0.3 收缩到 0，音符结束后消失。
#[test]
fn test_aura_shrinks_toward_note_end() {
    let mut positions = Vec::new();
    let mut widths = Vec::new();
    let mut last = 0u32;
    update_key_positions(128, &mut last, &mut positions, &mut widths);

    let uniform = MiditrailUniformGpu {
        tick: 0,
        ppq: 480,
        ticks_per_second: 960.0,
        fps: 60.0,
        ..MiditrailUniformGpu::default()
    };
    // 音符时长 4 秒（3840 tick），tick=3000 时剩余 840 tick < 1s
    let notes = vec![MiditrailNoteGpu {
        key: 60,
        start_tick: 0,
        end_tick: 3840,
        color_packed: 0xFFFF0000,
        track_idx: 0,
        velocity: 100,
        channel: 0,
        _padding: 0,
    }];
    let width = widths[60];
    let size_at = |tick: u32| {
        let uniform = MiditrailUniformGpu { tick, ..uniform };
        let active_keys = compute_active_keys(tick, &notes);
        let mut auras = Vec::new();
        build_aura_instances(
            &uniform,
            &notes,
            &active_keys,
            &positions,
            &widths,
            &mut auras,
        );
        auras.first().map_or(0.0, |a| a.size)
    };

    // 剩余 840/960s：系数 = (840/960)^0.3 × 0.5（闪光已归零）
    let expected = width * AURA_RING_SCALE * (840.0f32 / 960.0).powf(0.3) * 0.5;
    assert!(
        (size_at(3000) - expected).abs() < 1e-4,
        "最后 1 秒应开始收缩"
    );
    // 收缩单调递减
    assert!(size_at(3000) < size_at(2000), "临近结束光环应小于常态");
    // 音符结束后光环消失
    assert_eq!(size_at(3840), 0.0, "音符结束后光环应消失");
    assert_eq!(size_at(4000), 0.0);
}

/// 同键多个音符：取光晕系数最大值（后一个音符的按下闪光叠加在常态之上）。
#[test]
fn test_aura_takes_max_over_notes_on_key() {
    let mut positions = Vec::new();
    let mut widths = Vec::new();
    let mut last = 0u32;
    update_key_positions(128, &mut last, &mut positions, &mut widths);

    let uniform = MiditrailUniformGpu {
        tick: 0,
        ppq: 480,
        ticks_per_second: 960.0,
        fps: 60.0,
        ..MiditrailUniformGpu::default()
    };
    // 同一键两个音符：第一个已进入常态（闪光归零），第二个刚按下（闪光峰值）
    let notes = vec![
        MiditrailNoteGpu {
            key: 60,
            start_tick: 0,
            end_tick: 4800,
            color_packed: 0xFFFF0000,
            track_idx: 0,
            velocity: 100,
            channel: 0,
            _padding: 0,
        },
        MiditrailNoteGpu {
            key: 60,
            start_tick: 480,
            end_tick: 960,
            color_packed: 0x00FF00FF,
            track_idx: 1,
            velocity: 100,
            channel: 1,
            _padding: 0,
        },
    ];
    let tick = 480; // 第二个音符刚按下
    let active_keys = compute_active_keys(tick, &notes);
    let mut auras = Vec::new();
    build_aura_instances(
        &uniform,
        &notes,
        &active_keys,
        &positions,
        &widths,
        &mut auras,
    );
    assert_eq!(auras.len(), 1);
    let width = widths[60];
    // 第一个音符常态 0.5，第二个刚按下 0.5 + 100/600 → 取最大
    let expected = width * AURA_RING_SCALE * (0.5 + 100.0 / 600.0);
    assert!(
        (auras[0].size - expected).abs() < 1e-4,
        "同键多音符应取光晕系数最大值"
    );
}

/// 未开始的音符（start > tick）与已结束的音符不产生光环。
#[test]
fn test_aura_skips_future_and_ended_notes() {
    let mut positions = Vec::new();
    let mut widths = Vec::new();
    let mut last = 0u32;
    update_key_positions(128, &mut last, &mut positions, &mut widths);

    let uniform = MiditrailUniformGpu {
        tick: 100,
        ppq: 480,
        ticks_per_second: 960.0,
        fps: 60.0,
        ..MiditrailUniformGpu::default()
    };
    let notes = vec![
        MiditrailNoteGpu {
            key: 60,
            start_tick: 200, // 尚未开始
            end_tick: 400,
            color_packed: 0xFFFF0000,
            track_idx: 0,
            velocity: 100,
            channel: 0,
            _padding: 0,
        },
        MiditrailNoteGpu {
            key: 62,
            start_tick: 0,
            end_tick: 100, // 恰好结束（end <= tick）
            color_packed: 0xFFFF0000,
            track_idx: 0,
            velocity: 100,
            channel: 0,
            _padding: 0,
        },
        MiditrailNoteGpu {
            key: 64,
            start_tick: 0, // 正在发声
            end_tick: 200,
            color_packed: 0xFFFF0000,
            track_idx: 0,
            velocity: 100,
            channel: 0,
            _padding: 0,
        },
    ];
    let active_keys = compute_active_keys(uniform.tick, &notes);
    let mut auras = Vec::new();
    build_aura_instances(
        &uniform,
        &notes,
        &active_keys,
        &positions,
        &widths,
        &mut auras,
    );
    assert_eq!(auras.len(), 1, "仅正在发声的音符产生光环");
    assert_eq!(auras[0].pos, positions[64] + widths[64] * 0.5);
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

#[test]
fn test_note_instances_clipped_at_z_far_distance() {
    let mut positions = Vec::new();
    let mut widths = Vec::new();
    let mut last = 0u32;
    update_key_positions(128, &mut last, &mut positions, &mut widths);

    let uniform = MiditrailUniformGpu {
        z_far_distance: 3.0,
        ..MiditrailUniformGpu::default()
    };
    // 音符从当前 tick 开始，长度刚好覆盖整个视口，因此无裁剪时会延伸到 -SCENE_DEPTH。
    let notes = vec![MiditrailNoteGpu {
        key: 60,
        start_tick: 0,
        end_tick: 480 * 16,
        color_packed: 0xFF0000FF,
        track_idx: 0,
        velocity: 100,
        channel: 0,
        _padding: 0,
    }];
    let mut out = Vec::new();
    build_note_instances(&uniform, &notes, &positions, &widths, &mut out);
    assert_eq!(out.len(), 1);
    // 立方体锚定在 translation，向 +Z 方向延伸 scale[2]，因此后端直接为 translation[2]。
    let back_z = out[0].translation[2];
    let expected_z_far = NOTE_Z_OFFSET - uniform.z_far_distance;
    assert!(
        (back_z - expected_z_far).abs() < 1e-5,
        "音符应被截断到 Z 远平面，期望 {expected_z_far}, 实际 {back_z}"
    );
}

/// 窗口缩放等价性：2.0× 收集窗口（旧行为）与 1.0× 窗口（新行为）输入下，
/// `build_note_instances` 输出必须完全一致——即缩小收集窗口不漏掉任何可见音符。
#[test]
fn test_note_instances_window_scale_equivalence() {
    let mut positions = Vec::new();
    let mut widths = Vec::new();
    let mut last = 0u32;
    update_key_positions(128, &mut last, &mut positions, &mut widths);

    let uniform = MiditrailUniformGpu {
        tick: 1_000_000,
        ppq: 480,
        speed: 1.0,
        z_far_distance: 7.5, // 默认：z_far_scale = 1.0
        ..MiditrailUniformGpu::default()
    };
    // span = ppq*4 * (4/speed) = 7680
    let span = 7680u32;

    // 边界音符：分布在 [tick, tick + 2.0×span] 区间
    let mut notes = Vec::new();
    for (i, off) in [
        0u32,           // 正好当前 tick
        span / 4,       // 0.25×
        span / 2,       // 0.5×
        span * 3 / 4,   // 0.75×
        span,           // 1.0×（窗口边界，新行为恰好覆盖）
        span * 6 / 5,   // 1.2×（新行为窗口外）
        span * 3 / 2,   // 1.5×
        span * 19 / 10, // 1.9×
        span * 2,       // 2.0×（旧行为窗口边界）
    ]
    .into_iter()
    .enumerate()
    {
        notes.push(MiditrailNoteGpu {
            key: 40 + i as u32,
            start_tick: uniform.tick + off,
            end_tick: uniform.tick + off + 240,
            color_packed: 0xFF0000FF,
            track_idx: 0,
            velocity: 100,
            channel: 0,
            _padding: 0,
        });
    }

    // 1.0× 窗口输入：只包含 start_tick <= tick + 1.0×span 的音符
    let notes_1x: Vec<MiditrailNoteGpu> = notes
        .iter()
        .filter(|n| n.start_tick <= uniform.tick + span)
        .copied()
        .collect();
    assert!(notes_1x.len() < notes.len(), "1.0× 窗口应裁剪掉部分音符");

    let mut out_full = Vec::new();
    build_note_instances(&uniform, &notes, &positions, &widths, &mut out_full);
    let mut out_1x = Vec::new();
    build_note_instances(&uniform, &notes_1x, &positions, &widths, &mut out_1x);

    assert_eq!(
        out_1x.len(),
        out_full.len(),
        "缩小收集窗口后可见音符数必须一致：1x={}, 2x={}",
        out_1x.len(),
        out_full.len()
    );
    for (a, b) in out_1x.iter().zip(out_full.iter()) {
        assert_eq!(a.translation, b.translation, "translation 必须一致");
        assert_eq!(a.scale, b.scale, "scale 必须一致");
        assert_eq!(a.color_packed, b.color_packed, "颜色必须一致");
    }
}

/// 排序等价性：u64 打包键 sort_unstable_by_key 与旧三键闭包 sort_by
/// 输出顺序必须完全一致（视觉零差异回归护栏）。
#[test]
fn test_note_instances_sort_key_equivalent() {
    let mut positions = Vec::new();
    let mut widths = Vec::new();
    let mut last = 0u32;
    update_key_positions(128, &mut last, &mut positions, &mut widths);

    let uniform = MiditrailUniformGpu {
        tick: 500_000,
        ppq: 480,
        speed: 1.0,
        z_far_distance: 7.5,
        ..MiditrailUniformGpu::default()
    };
    // 构造覆盖各种排序情形的音符：黑白键混合、远近交错、同深度（同 z）不同 key
    let mut notes = Vec::new();
    for i in 0..200u32 {
        let key = i % 88;
        let start = uniform.tick + i * 97 % 3_000;
        notes.push(MiditrailNoteGpu {
            key,
            start_tick: start,
            end_tick: start + 240,
            color_packed: 0xFF0000FF,
            track_idx: i % 4,
            velocity: 100,
            channel: i % 16,
            _padding: 0,
        });
    }
    // 完全同键（同 key + 同 start_tick + 同可见性）的和弦叠音：
    // 旧稳定排序按输入序，新排序键相同 → 必须保持输入序（稳定语义回归护栏）。
    // start_tick 设在 tick 附近保证可见（不被 is_visible_at 过滤）。
    for (i, (key, track)) in [(60u32, 1u32), (61, 2), (60, 3)].into_iter().enumerate() {
        let start = uniform.tick.saturating_sub(100) + (i as u32) * 50;
        notes.push(MiditrailNoteGpu {
            key,
            start_tick: start,
            end_tick: start + 240,
            color_packed: 0x00FF00FF,
            track_idx: track,
            velocity: 100,
            channel: 0,
            _padding: 0,
        });
    }

    // 参考：旧三键闭包排序
    let mut expected_instances = Vec::new();
    build_note_instances_old_style(
        &uniform,
        &notes,
        &positions,
        &widths,
        &mut expected_instances,
    );
    // 新实现
    let mut actual_instances = Vec::new();
    build_note_instances(&uniform, &notes, &positions, &widths, &mut actual_instances);

    assert_eq!(
        expected_instances.len(),
        actual_instances.len(),
        "实例数量必须一致"
    );
    for (i, (a, b)) in expected_instances
        .iter()
        .zip(actual_instances.iter())
        .enumerate()
    {
        assert_eq!(
            a.translation, b.translation,
            "第 {i} 个实例 translation 不一致"
        );
        assert_eq!(a.scale, b.scale, "第 {i} 个实例 scale 不一致");
        assert_eq!(a.color_packed, b.color_packed, "第 {i} 个实例颜色不一致");
        assert_eq!(a.is_key, b.is_key, "第 {i} 个实例 is_key 不一致");
    }
}

/// 旧三键闭包排序的参考实现（用于等价性回归测试）。
fn build_note_instances_old_style(
    uniform: &MiditrailUniformGpu,
    notes: &[MiditrailNoteGpu],
    key_positions: &[f32],
    key_widths: &[f32],
    out: &mut Vec<MiditrailInstanceGpu>,
) {
    let tick = uniform.tick;
    let ppq = uniform.ppq.max(1);
    let speed = uniform.speed.max(0.1);
    let ticks_per_measure = ppq * 4;
    let visible_measure_count = ((4.0 / speed).round()).max(1.0) as u32;
    let viewport_tick_span = (ticks_per_measure * visible_measure_count).max(1) as f32;
    let z_far_distance = uniform.z_far_distance.max(0.1);
    let z_far = NOTE_Z_OFFSET - z_far_distance;

    let mut entries: Vec<(u32, f32, MiditrailInstanceGpu)> = Vec::new();
    for note in notes {
        if !note.is_visible_at(tick) {
            continue;
        }
        let key = note.key as usize;
        if key >= key_positions.len() {
            continue;
        }
        let left = key_positions[key];
        let width = key_widths[key];
        let visible_start = note.start_tick.max(tick);
        let visible_end = note.end_tick;
        let z_start = NOTE_Z_OFFSET
            - ((visible_start.saturating_sub(tick)) as f32 / viewport_tick_span
                * MIDITRAIL_SCENE_DEPTH);
        let mut z_end = NOTE_Z_OFFSET
            - ((visible_end.saturating_sub(tick)) as f32 / viewport_tick_span
                * MIDITRAIL_SCENE_DEPTH);
        z_end = z_end.max(z_far);
        if z_end >= z_start {
            continue;
        }
        let z_center = (z_start + z_end) * 0.5;
        let z_length = z_start - z_end;
        let scale = [width * 0.92, NOTE_HEIGHT, z_length];
        let translation = [left + width * 0.04, NOTE_Y, z_center - z_length * 0.5];
        let color = if note.is_active_at(tick) {
            boost_color_packed(note.color_packed, 0.5)
        } else {
            note.color_packed
        };
        entries.push((
            note.key,
            z_start,
            MiditrailInstanceGpu::new(translation, scale, color, false, 0.0, 0.0),
        ));
    }
    entries.sort_by(|a, b| {
        let a_black = is_black_key(a.0);
        let b_black = is_black_key(b.0);
        a_black
            .cmp(&b_black)
            .then_with(|| a.1.total_cmp(&b.1))
            .then_with(|| a.0.cmp(&b.0))
    });
    out.extend(entries.into_iter().map(|(_, _, instance)| instance));
}

/// 性能基准：10 万密集音符（视频导出场景）下实例构建总耗时。
///
/// 该测试不断言耗时（CI 机器抖动不可控），仅输出测量数据供人工比对：
/// - 优化前基线：全量扫描 ×2 + 每帧 O(V log V) 排序
/// - 优化后目标：单次扫描 + 桶内有序（排序消除或降级）
///
/// 运行：`cargo test -p lumino-gfx test_miditrail_instances_bench -- --nocapture`
#[test]
fn test_miditrail_instances_bench() {
    use std::time::Instant;

    let mut positions = Vec::new();
    let mut widths = Vec::new();
    let mut last = 0u32;
    update_key_positions(128, &mut last, &mut positions, &mut widths);

    let uniform = MiditrailUniformGpu {
        tick: 1_000_000,
        ppq: 480,
        key_count: 128,
        speed: 1.0,
        z_far_distance: 7.5,
        fps: 60.0,
        ..MiditrailUniformGpu::default()
    };

    // 10 万音符：密集分布在旧收集窗口（2.0× span + TICK_SEARCH_BUFFER 下界）内，
    // 模拟 collect_visible_notes_for_gpu 的输出
    const N: usize = 100_000;
    let span = 7680u32;
    let window_start = uniform.tick.saturating_sub(19_200);
    let window_end = uniform.tick + span * 2; // 旧 2.0× 窗口
    let mut notes = Vec::with_capacity(N);
    for i in 0..N {
        let t = window_start + (i as u64 * (window_end - window_start) as u64 / N as u64) as u32;
        notes.push(MiditrailNoteGpu {
            key: i as u32 % 88,
            start_tick: t,
            end_tick: t + 240,
            color_packed: 0xFF0000FF,
            track_idx: 0,
            velocity: 100,
            channel: 0,
            _padding: 0,
        });
    }
    // 新窗口输入：1.0× span 上界（start_tick <= tick + span）
    let notes_1x: Vec<MiditrailNoteGpu> = notes
        .iter()
        .filter(|n| n.start_tick <= uniform.tick + span)
        .copied()
        .collect();
    eprintln!(
        "[miditrail_bench] 窗口缩放: 旧2.0×={} 音符, 新1.0×={} 音符 (-{:.0}%)",
        notes.len(),
        notes_1x.len(),
        (1.0 - notes_1x.len() as f64 / notes.len() as f64) * 100.0
    );

    // 热身
    for _ in 0..3 {
        let active_keys = compute_active_keys(uniform.tick, &notes_1x);
        let mut out = Vec::with_capacity(notes_1x.len());
        build_note_instances(&uniform, &notes_1x, &positions, &widths, &mut out);
        std::hint::black_box((active_keys, out));
    }

    const ITERS: u32 = 30;
    let mut t_active = 0u64;
    let mut t_build = 0u64;
    let mut t_build_1x = 0u64;
    let mut t_visible = 0u64;
    let mut t_sort = 0u64;
    let mut t_sort_x_total = 0u64;
    let mut t_sort_s_total = 0u64;
    for _ in 0..ITERS {
        let t0 = Instant::now();
        let active_keys = compute_active_keys(uniform.tick, &notes);
        t_active += t0.elapsed().as_micros() as u64;

        // 旧行为：2.0× 窗口全量输入
        let t1 = Instant::now();
        let mut out = Vec::with_capacity(notes.len());
        build_note_instances(&uniform, &notes, &positions, &widths, &mut out);
        t_build += t1.elapsed().as_micros() as u64;

        // 新行为：1.0× 窗口输入
        let t2 = Instant::now();
        let mut out2 = Vec::with_capacity(notes_1x.len());
        build_note_instances(&uniform, &notes_1x, &positions, &widths, &mut out2);
        t_build_1x += t2.elapsed().as_micros() as u64;

        // 内部拆解：遍历+可见过滤 vs 排序（仅可见实例参与排序）
        let mut entries: Vec<(u32, f32, MiditrailInstanceGpu)> = Vec::with_capacity(notes_1x.len());
        let t3 = Instant::now();
        let tick = uniform.tick;
        let ppq = uniform.ppq.max(1);
        let speed = uniform.speed.max(0.1);
        let vtm = (ppq * 4 * ((4.0 / speed).round()).max(1.0) as u32).max(1) as f32;
        let z_far = NOTE_Z_OFFSET - uniform.z_far_distance.max(0.1);
        let positions = &positions;
        let widths = &widths;
        for note in &notes_1x {
            if !note.is_visible_at(tick) {
                continue;
            }
            let key = note.key as usize;
            if key >= positions.len() {
                continue;
            }
            let left = positions[key];
            let width = widths[key];
            let visible_start = note.start_tick.max(tick);
            let visible_end = note.end_tick;
            let z_start = NOTE_Z_OFFSET
                - ((visible_start.saturating_sub(tick)) as f32 / vtm * MIDITRAIL_SCENE_DEPTH);
            let mut z_end = NOTE_Z_OFFSET
                - ((visible_end.saturating_sub(tick)) as f32 / vtm * MIDITRAIL_SCENE_DEPTH);
            z_end = z_end.max(z_far);
            if z_end >= z_start {
                continue;
            }
            let z_center = (z_start + z_end) * 0.5;
            let z_length = z_start - z_end;
            let scale = [width * 0.92, NOTE_HEIGHT, z_length];
            let translation = [left + width * 0.04, NOTE_Y, z_center - z_length * 0.5];
            let color = if note.is_active_at(tick) {
                boost_color_packed(note.color_packed, 0.5)
            } else {
                note.color_packed
            };
            entries.push((
                note.key,
                z_start,
                MiditrailInstanceGpu::new(translation, scale, color, false, 0.0, 0.0),
            ));
        }
        t_visible += t3.elapsed().as_micros() as u64;
        let t4 = Instant::now();
        entries.sort_by(|a, b| {
            let a_black = is_black_key(a.0);
            let b_black = is_black_key(b.0);
            a_black
                .cmp(&b_black)
                .then_with(|| a.1.total_cmp(&b.1))
                .then_with(|| a.0.cmp(&b.0))
        });
        t_sort += t4.elapsed().as_micros() as u64;
        std::hint::black_box(entries);

        // 方案 X：单一 u64 键 sort_unstable_by_key
        let mut entries_x: Vec<(u64, MiditrailInstanceGpu)> = Vec::new();
        let tick = uniform.tick;
        let ppq = uniform.ppq.max(1);
        let speed = uniform.speed.max(0.1);
        let vtm = (ppq * 4 * ((4.0 / speed).round()).max(1.0) as u32).max(1) as f32;
        let z_far = NOTE_Z_OFFSET - uniform.z_far_distance.max(0.1);
        for note in &notes_1x {
            if !note.is_visible_at(tick) {
                continue;
            }
            let key = note.key as usize;
            if key >= positions.len() {
                continue;
            }
            let left = positions[key];
            let width = widths[key];
            let visible_start = note.start_tick.max(tick);
            let visible_end = note.end_tick;
            let z_start = NOTE_Z_OFFSET
                - ((visible_start.saturating_sub(tick)) as f32 / vtm * MIDITRAIL_SCENE_DEPTH);
            let mut z_end = NOTE_Z_OFFSET
                - ((visible_end.saturating_sub(tick)) as f32 / vtm * MIDITRAIL_SCENE_DEPTH);
            z_end = z_end.max(z_far);
            if z_end >= z_start {
                continue;
            }
            let z_center = (z_start + z_end) * 0.5;
            let z_length = z_start - z_end;
            let scale = [width * 0.92, NOTE_HEIGHT, z_length];
            let translation = [left + width * 0.04, NOTE_Y, z_center - z_length * 0.5];
            let color = if note.is_active_at(tick) {
                boost_color_packed(note.color_packed, 0.5)
            } else {
                note.color_packed
            };
            // f32 位重排 → 可排序 u32（与 total_cmp 全序等价）：
            // 正数：最高位翻转；负数：全位翻转
            let zb = z_start.to_bits();
            let z_sortable = if zb & 0x8000_0000 != 0 {
                !zb
            } else {
                zb ^ 0x8000_0000
            };
            let packed = ((is_black_key(note.key) as u64) << 63)
                | ((z_sortable as u64) << 7)
                | (note.key as u64);
            entries_x.push((
                packed,
                MiditrailInstanceGpu::new(translation, scale, color, false, 0.0, 0.0),
            ));
        }
        let t5 = Instant::now();
        entries_x.sort_unstable_by_key(|(packed, _)| *packed);
        let t_sort_x = t5.elapsed().as_micros() as u64;
        std::hint::black_box(&entries_x);
        t_sort_x_total += t_sort_x;

        // 方案 X2：稳定 sort_by_key（保留旧稳定语义，避免同键叠音顺序不确定）
        let mut entries_s = entries_x.clone();
        let t6 = Instant::now();
        entries_s.sort_by_key(|(packed, _)| *packed);
        let t_sort_s = t6.elapsed().as_micros() as u64;
        std::hint::black_box(entries_s);
        t_sort_s_total += t_sort_s;

        std::hint::black_box((active_keys, out, out2));
    }
    eprintln!(
        "[miditrail_bench] active {:.2}ms | 旧2.0×build {:.2}ms | 新1.0×build {:.2}ms (省 {:.1}%) | 遍历+过滤 {:.2}ms | sort_by三键 {:.2}ms | unstable_by_key {:.2}ms | stable_by_key {:.2}ms",
        t_active as f64 / ITERS as f64 / 1000.0,
        t_build as f64 / ITERS as f64 / 1000.0,
        t_build_1x as f64 / ITERS as f64 / 1000.0,
        (1.0 - t_build_1x as f64 / t_build as f64) * 100.0,
        t_visible as f64 / ITERS as f64 / 1000.0,
        t_sort as f64 / ITERS as f64 / 1000.0,
        t_sort_x_total as f64 / ITERS as f64 / 1000.0,
        t_sort_s_total as f64 / ITERS as f64 / 1000.0
    );
}
