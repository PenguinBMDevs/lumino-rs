//! Aura 光晕环动画测试（按下闪光衰减 / 临近结束收缩 / 同键取最大 / 跳过未开始与已结束）

use super::*;

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
